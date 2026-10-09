//! Live updates: every change is broadcast to all open browsers over Server-Sent Events.
//!
//! Each event is `{kind, by, data}`, where `kind` is e.g. `task.moved`, `by` is the user who made
//! the change, and `data` is the changed row (or just its ids for deletions). Clients apply
//! events idempotently, so the tab that made the change can safely receive its own echo.
//!
//! A client that falls behind gets a `resync` event and refetches. Streams end when the server
//! shuts down, so graceful shutdown never waits on open browser tabs.

use std::{convert::Infallible, time::Duration};

use axum::{
    extract::State,
    response::sse::{self, KeepAlive, Sse},
};
use futures_util::{Stream, StreamExt as _, stream};
use serde::Serialize;
use serde_json::{Value, json};
use tokio::sync::{broadcast, watch};

use crate::{AppState, auth::CurrentUser};

/// Enough headroom for a burst of drags; a slower client resyncs instead of blocking anyone.
const BUFFER: usize = 256;

#[derive(Debug, Clone, Serialize)]
pub struct Event {
    pub kind: &'static str,
    pub by: i64,
    pub data: Value,
}

#[derive(Clone)]
pub struct Hub {
    tx: broadcast::Sender<Event>,
    shutdown: watch::Receiver<bool>,
}

impl Hub {
    /// The hub, plus the switch that ends every stream (flip it to `true` on shutdown).
    pub fn new() -> (Self, watch::Sender<bool>) {
        let (tx, _) = broadcast::channel(BUFFER);
        let (stop, shutdown) = watch::channel(false);
        (Self { tx, shutdown }, stop)
    }

    pub fn send(&self, kind: &'static str, by: i64, data: impl Serialize) {
        let data = serde_json::to_value(data).unwrap_or(Value::Null);
        // An error only means nobody is listening right now.
        let _ = self.tx.send(Event { kind, by, data });
    }

    #[cfg(test)]
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.tx.subscribe()
    }
}

/// `GET /api/events`
pub async fn stream(
    State(state): State<AppState>,
    CurrentUser(me): CurrentUser,
) -> Sse<impl Stream<Item = Result<sse::Event, Infallible>>> {
    let rx = state.events.tx.subscribe();
    let shutdown = state.events.shutdown.clone();
    let hello = sse::Event::default()
        .event("hello")
        .data(json!({ "user": me.id }).to_string());

    let events = stream::unfold((rx, shutdown), |(mut rx, mut shutdown)| async move {
        if *shutdown.borrow() {
            return None;
        }
        let next = tokio::select! {
            _ = shutdown.changed() => return None,
            next = rx.recv() => next,
        };
        let event = match next {
            Ok(e) => sse::Event::default()
                .event(e.kind)
                .data(json!({ "by": e.by, "data": e.data }).to_string()),
            Err(broadcast::error::RecvError::Lagged(_)) => {
                sse::Event::default().event("resync").data("{}")
            }
            Err(broadcast::error::RecvError::Closed) => return None,
        };
        Some((Ok(event), (rx, shutdown)))
    });

    Sse::new(stream::iter([Ok(hello)]).chain(events))
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(20)))
}
