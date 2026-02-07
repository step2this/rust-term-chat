//! Loopback transport for testing.
//!
//! Uses in-process [`tokio::sync::mpsc`] channels to simulate a network
//! connection between two peers. Created via [`LoopbackTransport::create_pair`],
//! which returns two connected endpoints — sending on one delivers to the other.

use tokio::sync::{Mutex, mpsc};

use super::{PeerId, Transport, TransportError, TransportType};

/// In-process transport backed by `tokio::sync::mpsc` channels.
///
/// Each `LoopbackTransport` holds a sender to push messages toward its
/// remote peer and a receiver to pull messages that the remote peer sent.
/// Use [`create_pair`](LoopbackTransport::create_pair) to get two connected endpoints.
pub struct LoopbackTransport {
    /// Identity of the local side.
    local_id: PeerId,
    /// Identity of the remote peer.
    remote_id: PeerId,
    /// Sender for outgoing messages (delivers to the remote's receiver).
    tx: mpsc::Sender<(PeerId, Vec<u8>)>,
    /// Receiver for incoming messages (fed by the remote's sender).
    rx: Mutex<mpsc::Receiver<(PeerId, Vec<u8>)>>,
}

impl LoopbackTransport {
    /// Create a pair of connected loopback transports.
    ///
    /// Messages sent by one end are received by the other. The `buffer`
    /// parameter controls the channel capacity for each direction.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use termchat::transport::loopback::LoopbackTransport;
    /// use termchat::transport::{PeerId, Transport};
    ///
    /// # async fn example() {
    /// let (alice, bob) = LoopbackTransport::create_pair(
    ///     PeerId::new("alice"),
    ///     PeerId::new("bob"),
    ///     32,
    /// );
    /// alice.send(&PeerId::new("bob"), b"hello").await.unwrap();
    /// let (from, data) = bob.recv().await.unwrap();
    /// assert_eq!(data, b"hello");
    /// # }
    /// ```
    pub fn create_pair(
        id_a: PeerId,
        id_b: PeerId,
        buffer: usize,
    ) -> (LoopbackTransport, LoopbackTransport) {
        let (tx_a, rx_a) = mpsc::channel(buffer);
        let (tx_b, rx_b) = mpsc::channel(buffer);

        let a = LoopbackTransport {
            local_id: id_a.clone(),
            remote_id: id_b.clone(),
            tx: tx_b, // A sends into B's receiver
            rx: Mutex::new(rx_a),
        };

        let b = LoopbackTransport {
            local_id: id_b,
            remote_id: id_a,
            tx: tx_a, // B sends into A's receiver
            rx: Mutex::new(rx_b),
        };

        (a, b)
    }
}

impl Transport for LoopbackTransport {
    async fn send(&self, peer: &PeerId, payload: &[u8]) -> Result<(), TransportError> {
        if *peer != self.remote_id {
            return Err(TransportError::Unreachable(peer.clone()));
        }

        self.tx
            .send((self.local_id.clone(), payload.to_vec()))
            .await
            .map_err(|_| TransportError::ConnectionClosed)
    }

    async fn recv(&self) -> Result<(PeerId, Vec<u8>), TransportError> {
        let mut rx = self.rx.lock().await;
        rx.recv().await.ok_or(TransportError::ConnectionClosed)
    }

    fn is_connected(&self, peer: &PeerId) -> bool {
        *peer == self.remote_id && !self.tx.is_closed()
    }

    fn transport_type(&self) -> TransportType {
        TransportType::Loopback
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn send_recv_round_trip() {
        let (alice, bob) =
            LoopbackTransport::create_pair(PeerId::new("alice"), PeerId::new("bob"), 32);

        let payload = b"hello, world!";
        alice.send(&PeerId::new("bob"), payload).await.unwrap();

        let (from, data) = bob.recv().await.unwrap();
        assert_eq!(from, PeerId::new("alice"));
        assert_eq!(data, payload);
    }

    #[tokio::test]
    async fn send_to_wrong_peer_returns_unreachable() {
        let (alice, _bob) =
            LoopbackTransport::create_pair(PeerId::new("alice"), PeerId::new("bob"), 32);

        let result = alice.send(&PeerId::new("charlie"), b"hi").await;
        assert!(matches!(result, Err(TransportError::Unreachable(_))));
    }

    #[tokio::test]
    async fn is_connected_reflects_channel_state() {
        let (alice, bob) =
            LoopbackTransport::create_pair(PeerId::new("alice"), PeerId::new("bob"), 32);

        assert!(alice.is_connected(&PeerId::new("bob")));
        assert!(!alice.is_connected(&PeerId::new("charlie")));

        // Drop bob's side — alice's sender should detect the closed channel.
        drop(bob);
        assert!(!alice.is_connected(&PeerId::new("bob")));
    }

    #[tokio::test]
    async fn send_after_remote_drop_returns_connection_closed() {
        let (alice, bob) =
            LoopbackTransport::create_pair(PeerId::new("alice"), PeerId::new("bob"), 32);

        drop(bob);

        let result = alice.send(&PeerId::new("bob"), b"hi").await;
        assert!(matches!(result, Err(TransportError::ConnectionClosed)));
    }

    #[tokio::test]
    async fn recv_after_remote_drop_returns_connection_closed() {
        let (alice, bob) =
            LoopbackTransport::create_pair(PeerId::new("alice"), PeerId::new("bob"), 32);

        drop(bob);

        let result = alice.recv().await;
        assert!(matches!(result, Err(TransportError::ConnectionClosed)));
    }

    #[tokio::test]
    async fn transport_type_is_loopback() {
        let (alice, _bob) =
            LoopbackTransport::create_pair(PeerId::new("alice"), PeerId::new("bob"), 32);
        assert_eq!(alice.transport_type(), TransportType::Loopback);
    }

    #[tokio::test]
    async fn bidirectional_communication() {
        let (alice, bob) =
            LoopbackTransport::create_pair(PeerId::new("alice"), PeerId::new("bob"), 32);

        // Alice sends to Bob.
        alice
            .send(&PeerId::new("bob"), b"from alice")
            .await
            .unwrap();
        let (from, data) = bob.recv().await.unwrap();
        assert_eq!(from, PeerId::new("alice"));
        assert_eq!(data, b"from alice");

        // Bob sends to Alice.
        bob.send(&PeerId::new("alice"), b"from bob").await.unwrap();
        let (from, data) = alice.recv().await.unwrap();
        assert_eq!(from, PeerId::new("bob"));
        assert_eq!(data, b"from bob");
    }

    #[tokio::test]
    async fn multiple_messages_preserve_order() {
        let (alice, bob) =
            LoopbackTransport::create_pair(PeerId::new("alice"), PeerId::new("bob"), 32);

        for i in 0u32..10 {
            alice
                .send(&PeerId::new("bob"), &i.to_le_bytes())
                .await
                .unwrap();
        }

        for i in 0u32..10 {
            let (_from, data) = bob.recv().await.unwrap();
            let received = u32::from_le_bytes(data.try_into().unwrap());
            assert_eq!(received, i);
        }
    }
}
