use std::net::UdpSocket;
use std::time::Duration;

/// Verify that the core relay mechanic works: a packet sent to the relay socket
/// is forwarded unchanged to the target socket.
#[test]
fn udp_relay_forwards_bytes_unchanged() {
    // Use OS-assigned ports to avoid collisions with other test runs.
    let simhub_socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    let simhub_port = simhub_socket.local_addr().unwrap().port();
    simhub_socket
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();

    let relay_socket = UdpSocket::bind("0.0.0.0:0").unwrap();
    let relay_port = relay_socket.local_addr().unwrap().port();
    relay_socket
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();

    let test_payload = b"sim-relay-test-deadbeef-1234";

    // Send from a background thread so recv_from doesn't race with the send.
    std::thread::spawn(move || {
        let sender = UdpSocket::bind("0.0.0.0:0").unwrap();
        std::thread::sleep(Duration::from_millis(10));
        sender
            .send_to(test_payload, ("127.0.0.1", relay_port))
            .unwrap();
    });

    // Relay: receive then forward.
    let fwd_socket = UdpSocket::bind("0.0.0.0:0").unwrap();
    let mut buf = [0u8; 65_536];
    let (len, _src) = relay_socket.recv_from(&mut buf).unwrap();
    fwd_socket
        .send_to(&buf[..len], ("127.0.0.1", simhub_port))
        .unwrap();

    // Verify the payload arrived intact.
    let mut recv_buf = [0u8; 65_536];
    let (recv_len, _) = simhub_socket.recv_from(&mut recv_buf).unwrap();
    assert_eq!(&recv_buf[..recv_len], test_payload);
}

/// Verify that multiple packets are all relayed correctly.
#[test]
fn udp_relay_multiple_packets() {
    let dst_socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    let dst_port = dst_socket.local_addr().unwrap().port();
    dst_socket
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();

    let relay_socket = UdpSocket::bind("0.0.0.0:0").unwrap();
    let relay_port = relay_socket.local_addr().unwrap().port();
    relay_socket
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();

    let payloads: &[&[u8]] = &[b"packet-one", b"packet-two", b"packet-three"];

    let payloads_clone: Vec<Vec<u8>> = payloads.iter().map(|p| p.to_vec()).collect();
    std::thread::spawn(move || {
        let sender = UdpSocket::bind("0.0.0.0:0").unwrap();
        std::thread::sleep(Duration::from_millis(10));
        for p in &payloads_clone {
            sender.send_to(p, ("127.0.0.1", relay_port)).unwrap();
            std::thread::sleep(Duration::from_millis(1));
        }
    });

    let fwd_socket = UdpSocket::bind("0.0.0.0:0").unwrap();
    let mut buf = [0u8; 65_536];
    let mut received: Vec<Vec<u8>> = Vec::new();

    for _ in 0..payloads.len() {
        let (len, _) = relay_socket.recv_from(&mut buf).unwrap();
        fwd_socket
            .send_to(&buf[..len], ("127.0.0.1", dst_port))
            .unwrap();
        let mut rbuf = [0u8; 65_536];
        let (rlen, _) = dst_socket.recv_from(&mut rbuf).unwrap();
        received.push(rbuf[..rlen].to_vec());
    }

    for (expected, actual) in payloads.iter().zip(received.iter()) {
        assert_eq!(*expected, actual.as_slice());
    }
}
