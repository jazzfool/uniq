//! Thread-safe (`Arc`-based) queue/listener API.

use {
    super::*,
    crate::pack::{Packable, Unpackable},
    reclutch_event::{self as event, prelude::*},
    std::{any::Any, collections::HashMap, sync::Arc},
};

type QueueEvent<Id> = Event<Id, Arc<dyn any::Any + Send + Sync>>;

/// An adapter over an underlying listener in which a list of handlers are dispatched based on event type and ID.
///
/// This will not dispatch automatically. [`dispatch`](Listener::dispatch) must be called at regular intervals to handle events.
///
/// This type cannot be constructed directly. Invoke the `listen` method on the corresponding queue to create a new `Listener`.
pub struct Listener<Id: Clone + std::hash::Hash + Eq, T: Packable> {
    handlers:
        HashMap<(Id, any::TypeId), Arc<dyn Fn(<T as Packable>::Packed, &dyn Any) + Send + Sync>>,
    listener: event::ts::Listener<QueueEvent<Id>>,
}

impl<Id: Clone + std::hash::Hash + Eq, T: Packable> Listener<Id, T> {
    /// Adds a handler to `self` and returns `Self`.
    ///
    /// `id` marks the source ID. The type of the third parameter of the handler is the event type.
    /// Both of these will be used to match correct events.
    ///
    /// If the ID and event type are already being handled, the handler will be replaced.
    pub fn and_on<'a, E: Send + Sync + 'static, P: 'a>(
        mut self,
        id: Id,
        handler: impl Fn(P, &E) + Send + Sync + 'static,
    ) -> Self
    where
        T: Unpackable<'a, Unpacked = P>,
    {
        self.on(id, handler);
        self
    }

    /// Adds a handler.
    ///
    /// `id` marks the source ID. The type of the third parameter of the handler is the event type.
    /// Both of these will be used to match correct events.
    ///
    /// If the ID and event type are already being handled, the handler will be replaced.
    pub fn on<'a, E: Send + Sync + 'static, P: 'a>(
        &mut self,
        id: Id,
        handler: impl Fn(P, &E) + Send + Sync + 'static,
    ) -> (Id, any::TypeId)
    where
        T: Unpackable<'a, Unpacked = P>,
    {
        let k = (id, any::TypeId::of::<E>());
        self.handlers.insert(
            k.clone(),
            Arc::new(move |packed, ev| handler(T::unpack(packed), ev.downcast_ref::<E>().unwrap())),
        );
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
    pub fn dispatch(&mut self, it: <T as Unpackable<'_>>::Unpacked)
    where
        T: for<'a> Unpackable<'a>,
    {
        let packed = T::pack(it);
        for event in self.listener.peek() {
            if let Some(handler) = self.handlers.get_mut(&(event.id.clone(), event.type_id)) {
                handler(packed, event.data.as_ref());
            }
        }
    }
}

/// Thread-safe heterogenous queue.
///
/// In order to process events, specialized listeners need to be created via [`listen`](Queue::listen).
pub struct Queue<Id: Clone + std::hash::Hash + Eq + 'static = u64> {
    q: event::ts::Queue<QueueEvent<Id>>,
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
    /// This is a convenience function for [`emit_arc`](Queue::emit_arc), in which the event is moved into an atomically reference-counted pointer.
    pub fn emit<E: Send + Sync + 'static>(&self, id: Id, event: E) {
        self.emit_arc(id, Arc::new(event));
    }

    /// Emits an event already wrapped in an atomically reference-counted pointer.
    pub fn emit_arc<E: Send + Sync + 'static>(&self, id: Id, event: Arc<E>) {
        self.q.emit_owned(Event {
            id,
            type_id: any::TypeId::of::<E>(),
            data: event,
        });
    }

    /// Emits an event with an unknown type ([`Any`](std::any::Any)) wrapped in an atomically reference-counted pointer.
    pub fn emit_dyn(&self, id: Id, event: Arc<dyn any::Any + Send + Sync>) {
        self.q.emit_owned(Event {
            id,
            type_id: (*event).type_id(),
            data: event,
        });
    }

    /// Creates a new listener.
    ///
    /// Events emitted prior to this invocation will not be visible to the listener.
    pub fn listen<T: Packable>(&self) -> EventListener<T, Id> {
        EventListener {
            handlers: Default::default(),
            listener: self.q.listen(),
        }
    }
}

/// Thread-safe listener associated with an [`Queue`](Queue).
pub type EventListener<T, Id = u64> = Listener<Id, T>;

#[cfg(test)]
mod tests {
    use super::*;

    struct EventA;
    struct EventB;

    #[test]
    fn test_event_dispatch() {
        let queue: Queue = Queue::new();

        let mut l0 = queue
            .listen::<Write<Vec<&'static str>>>()
            .and_on(0, |o, _: &EventA| {
                o.push("a0");
            })
            .and_on(1, |o, _: &EventA| {
                o.push("a1");
            })
            .and_on(0, |o, _: &EventB| {
                o.push("b0");
            });

        let mut l1 = queue.listen::<Write<Vec<&'static str>>>();
        l1.on(0, |o: &mut Vec<&'static str>, _: &EventB| {
            o.push("b0");
        });

        queue.emit(1, EventA);
        queue.emit_arc(0, Arc::new(EventB));
        queue.emit_dyn(0, Arc::new(EventA));
        queue.emit(0, EventB);

        let mut v0 = Vec::new();
        let mut v1 = Vec::new();

        l0.dispatch(&mut v0);
        l1.dispatch(&mut v1);

        assert_eq!(&v0, &["a1", "b0", "a0", "b0"]);
        assert_eq!(&v1, &["b0", "b0"]);
    }

    #[test]
    fn test_event_dispatch_threaded() {
        use std::sync::Mutex;

        let queue: Queue = Queue::new();

        let mut l0 = queue
            .listen::<Write<Vec<&'static str>>>()
            .and_on(0, |o, _: &EventA| {
                o.push("a0");
            })
            .and_on(1, |o, _: &EventA| {
                o.push("a1");
            })
            .and_on(0, |o, _: &EventB| {
                o.push("b0");
            });

        let mut l1 = queue.listen::<Write<Vec<&'static str>>>();
        l1.on(0, |o, _: &EventB| {
            o.push("b0");
        });

        queue.emit(1, EventA);
        queue.emit_arc(0, Arc::new(EventB));
        std::thread::spawn(move || {
            queue.emit_dyn(0, Arc::new(EventA));
            queue.emit(0, EventB);
        })
        .join()
        .unwrap();

        let mut v0 = Vec::new();
        let v1 = Arc::new(Mutex::new(Vec::new()));

        l0.dispatch(&mut v0);

        let v1b = Arc::clone(&v1);
        std::thread::spawn(move || l1.dispatch(&mut *v1b.lock().unwrap()))
            .join()
            .unwrap();

        assert_eq!(&v0, &["a1", "b0", "a0", "b0"]);
        assert_eq!(*v1.lock().unwrap(), &["b0", "b0"]);
    }
}
