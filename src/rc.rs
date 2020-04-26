//! Non-thread-safe (`Rc`-based) queue/listener API.

use {
    super::*,
    reclutch_event::{self as event, prelude::*},
    std::{collections::HashMap, rc::Rc},
};

trait DynHandler<O, A> {
    fn handle(&mut self, o: &mut O, a: &mut A, e: &dyn any::Any);
}

type QueueEvent<Id> = Event<Id, Rc<dyn any::Any>>;

struct Handler<O, A, E, F: FnMut(&mut O, &mut A, &E)>(F, std::marker::PhantomData<(O, A, E)>);

impl<O, A, E: 'static, F: FnMut(&mut O, &mut A, &E)> DynHandler<O, A> for Handler<O, A, E, F> {
    fn handle(&mut self, o: &mut O, a: &mut A, e: &dyn any::Any) {
        (self.0)(o, a, e.downcast_ref::<E>().unwrap())
    }
}

/// An adapter over an underlying listener in which a list of handlers are dispatched based on event type and ID.
///
/// This will not dispatch automatically. [`dispatch`](Listener::dispatch) must be called at regular intervals to handle events.
///
/// This type cannot be constructed directly. Invoke the `listen` method on the corresponding queue to create a new `Listener`.
pub struct Listener<Id: Clone + std::hash::Hash + Eq, O: 'static, A: 'static> {
    handlers: HashMap<(Id, any::TypeId), Box<dyn DynHandler<O, A>>>,
    listener: event::RcEventListener<QueueEvent<Id>>,
}

impl<Id: Clone + std::hash::Hash + Eq, O: 'static, A: 'static> Listener<Id, O, A> {
    /// Adds a handler to `self` and returns `Self`.
    ///
    /// `id` marks the source ID. The type of the third parameter of the handler is the event type.
    /// Both of these will be used to match correct events.
    ///
    /// If the ID and event type are already being handled, the handler will be replaced.
    pub fn and_on<E: 'static>(
        mut self,
        id: Id,
        handler: impl FnMut(&mut O, &mut A, &E) + 'static,
    ) -> Self {
        self.on(id, handler);
        self
    }

    /// Adds a handler.
    ///
    /// `id` marks the source ID. The type of the third parameter of the handler is the event type.
    /// Both of these will be used to match correct events.
    ///
    /// If the ID and event type are already being handled, the handler will be replaced.
    pub fn on<E: 'static>(
        &mut self,
        id: Id,
        handler: impl FnMut(&mut O, &mut A, &E) + 'static,
    ) -> (Id, any::TypeId) {
        let k = (id, any::TypeId::of::<E>());
        self.handlers
            .insert(k.clone(), Box::new(Handler(handler, Default::default())));
        k
    }

    /// Removes a handler which matches a specific `id` and event type.
    pub fn remove<E: 'static>(&mut self, id: Id) -> bool {
        self.handlers
            .remove(&(id, any::TypeId::of::<E>()))
            .is_some()
    }

    /// Returns `true` if there is a handler handling `id` and event type `E`.
    pub fn contains<E: 'static>(&self, id: Id) -> bool {
        self.handlers.contains_key(&(id, any::TypeId::of::<E>()))
    }

    /// Processes incoming events and invokes the corresponding handler.
    pub fn dispatch(&mut self, o: &mut O, a: &mut A) {
        let handlers = &mut self.handlers;
        self.listener.with(|events| {
            for event in events {
                if let Some(handler) = handlers.get_mut(&(event.id.clone(), event.type_id)) {
                    handler.handle(o, a, event.data.as_ref());
                }
            }
        });
    }
}

/// Non-thread-safe heterogenous queue.
///
/// In order to process events, specialized listeners need to be created via [`listen`](Queue::listen).
#[derive(Debug)]
pub struct Queue<Id: Clone + std::hash::Hash + Eq + 'static = u64> {
    q: event::RcEventQueue<QueueEvent<Id>>,
}

impl<Id: Clone + std::hash::Hash + Eq + 'static> Default for Queue<Id> {
    fn default() -> Self {
        Queue {
            q: Default::default(),
        }
    }
}

impl<Id: Clone + std::hash::Hash + Eq + 'static> Queue<Id> {
    /// Creates a new [`Queue`](Queue). Equivalent to `Queue::default()`.
    #[inline]
    pub fn new() -> Self {
        Default::default()
    }

    /// Emits an event.
    ///
    /// This is a convenience function for [`emit_rc`](Queue::emit_rc), in which the event is moved into a reference-counted pointer.
    pub fn emit<E: 'static>(&self, id: Id, event: E) {
        self.emit_rc(id, Rc::new(event));
    }

    /// Emits an event already wrapped in a reference-counted pointer.
    pub fn emit_rc<E: 'static>(&self, id: Id, event: Rc<E>) {
        self.q.emit_owned(Event {
            id,
            type_id: any::TypeId::of::<E>(),
            data: event,
        });
    }

    /// Emits an event with an unknown type ([`Any`](std::any::Any)) wrapped in an atomically reference-counted pointer.
    pub fn emit_dyn(&self, id: Id, event: Rc<dyn any::Any>) {
        self.q.emit_owned(Event {
            id,
            type_id: event.type_id(),
            data: event,
        });
    }

    /// Creates a new listener.
    ///
    /// Events emitted prior to this invocation will not be visible to the listener.
    pub fn listen<O: 'static, A: 'static>(&self) -> EventListener<O, A, Id> {
        EventListener {
            handlers: Default::default(),
            listener: self.q.listen(),
        }
    }
}

/// Non-thread-safe listener associated with an [`Queue`](Queue).
pub type EventListener<O, A, Id = u64> = Listener<Id, O, A>;

#[cfg(test)]
mod tests {
    use super::*;

    struct EventA;
    struct EventB;

    #[test]
    fn test_event_dispatch() {
        let queue: Queue = Queue::new();

        let mut l0 = queue
            .listen()
            .and_on(0, |o: &mut Vec<&'static str>, _: &mut (), _: &EventA| {
                o.push("a0");
            })
            .and_on(1, |o: &mut Vec<&'static str>, _, _: &EventA| {
                o.push("a1");
            })
            .and_on(0, |o: &mut Vec<&'static str>, _, _: &EventB| {
                o.push("b0");
            });

        let mut l1 = queue.listen();
        l1.on(0, |o: &mut Vec<&'static str>, _: &mut (), _: &EventB| {
            o.push("b0");
        });

        queue.emit(1, EventA);
        queue.emit_rc(0, Rc::new(EventB));
        queue.emit_dyn(0, Rc::new(EventA));
        queue.emit(0, EventB);

        let mut v0 = Vec::new();
        let mut v1 = Vec::new();

        l0.dispatch(&mut v0, &mut ());
        l1.dispatch(&mut v1, &mut ());

        assert_eq!(&v0, &["a1", "b0", "a0", "b0"]);
        assert_eq!(&v1, &["b0", "b0"]);
    }
}
