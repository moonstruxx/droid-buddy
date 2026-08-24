//! Synchronous observer event bus connecting model, graph, and renderer
//! (design D6). Deliberately minimal: an event enum, inline dispatch to
//! subscribers, no queueing, no async, single-threaded.
//!
//! The bus decouples the re-solve triggers (patch load, node move) from both
//! the layout solver and the renderer, and gives topology errors a path to the
//! status surface. Subscribers are plain `FnMut` closures invoked in
//! subscription order on `dispatch`; a `Subscription` handle allows removal.

use crate::graph::{NodeId, TopologyIssue};

/// Events the bus can carry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// A node was dragged to a new position (task 4.3 emits this after a
    /// local re-settle).
    NodeMoved(NodeId),
    /// The graph was (re)built and re-solved; subscribers re-render.
    GraphRebuilt,
    /// A topology-validation finding; carries the offending cable/issue.
    TopologyError(TopologyIssue),
}

/// Handle identifying one registered subscriber, for later removal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Subscription(usize);

/// Boxed subscriber callback, aliased so the subscriber list stays readable
/// (clippy `type_complexity`).
type Subscriber = Box<dyn FnMut(&Event)>;

/// Minimal synchronous event bus.
#[derive(Default)]
pub struct EventBus {
    subscribers: Vec<Option<Subscriber>>,
}

impl EventBus {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a subscriber. The returned handle is used by `unsubscribe`.
    pub fn subscribe<F>(&mut self, subscriber: F) -> Subscription
    where
        F: FnMut(&Event) + 'static,
    {
        let id = self.subscribers.len();
        self.subscribers.push(Some(Box::new(subscriber)));
        Subscription(id)
    }

    /// Remove a previously registered subscriber. A stale or duplicate handle
    /// is a no-op.
    pub fn unsubscribe(&mut self, subscription: Subscription) {
        if let Some(slot) = self.subscribers.get_mut(subscription.0) {
            *slot = None;
        }
    }

    /// Notify every live subscriber, inline and in subscription order. With no
    /// subscribers this is a no-op.
    pub fn dispatch(&mut self, event: &Event) {
        for subscriber in self.subscribers.iter_mut().flatten() {
            subscriber(event);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn issue(cable: &str) -> TopologyIssue {
        TopologyIssue {
            cable: cable.to_string(),
            severity: crate::graph::TopologySeverity::Warning,
            message: String::from("test issue"),
        }
    }

    #[test]
    fn subscriber_receives_notification() {
        let mut bus = EventBus::new();
        let received = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let store = std::rc::Rc::clone(&received);
        bus.subscribe(move |event| store.borrow_mut().push(event.clone()));

        bus.dispatch(&Event::GraphRebuilt);

        assert_eq!(*received.borrow(), vec![Event::GraphRebuilt]);
    }

    #[test]
    fn events_are_delivered_in_dispatch_order() {
        let mut bus = EventBus::new();
        let order = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let store = std::rc::Rc::clone(&order);
        bus.subscribe(move |event| {
            store.borrow_mut().push(match event {
                Event::NodeMoved(_) => 0,
                Event::GraphRebuilt => 1,
                Event::TopologyError(_) => 2,
            });
        });

        bus.dispatch(&Event::GraphRebuilt);
        bus.dispatch(&Event::NodeMoved((String::from("osc"), 0)));
        bus.dispatch(&Event::TopologyError(issue("_BUS")));
        bus.dispatch(&Event::GraphRebuilt);

        assert_eq!(*order.borrow(), vec![1, 0, 2, 1]);
    }

    #[test]
    fn dispatch_with_no_subscribers_is_a_noop() {
        let mut bus = EventBus::new();
        bus.dispatch(&Event::GraphRebuilt);
        bus.dispatch(&Event::NodeMoved((String::from("osc"), 0)));
        bus.dispatch(&Event::TopologyError(issue("_BUS")));
    }

    #[test]
    fn unsubscribe_removes_a_subscriber() {
        // A `'static` subscriber captures shared state via `Rc`, not a local.
        let hits = std::rc::Rc::new(std::cell::Cell::new(0));
        let mut bus = EventBus::new();
        let hits_clone = std::rc::Rc::clone(&hits);
        let sub = bus.subscribe(move |_| hits_clone.set(hits_clone.get() + 1));

        bus.dispatch(&Event::GraphRebuilt);
        assert_eq!(hits.get(), 1);

        bus.unsubscribe(sub);
        bus.dispatch(&Event::GraphRebuilt);
        assert_eq!(hits.get(), 1, "unsubscribed subscriber must not fire again");
    }
}
