pub mod arc;
pub mod rc;

use std::any;

#[cfg(feature = "id")]
pub mod id;

/// Simple event type which stores the event type ID, the source ID and the event data itself.
#[derive(Debug, Clone)]
pub struct Event<Id: Clone, Data: Clone + 'static> {
    id: Id,
    type_id: any::TypeId,
    data: Data,
}
