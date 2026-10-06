use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

async fn endpoint(server: bool) -> Endpoint {
    Endpoint::builder(presets::Minimal)
        .clear_ip_transports()
        .bind_addr("127.0.0.1:0")
        .unwrap()
        .alpns(if server { vec![ALPN.to_vec()] } else { vec![] })
        .bind()
        .await
        .unwrap()
}

async fn request(
    session: Arc<session::ClientSession>,
    client: &Endpoint,
    server: &Endpoint,
    method: &str,
    budget: Duration,
) -> Result<IrohHttpResponse> {
    let addr = EndpointAddr::new(server.id()).with_ip_addr(server.bound_sockets()[0]);
    timeout(
        budget,
        exchange(
            session,
            client,
            addr,
            method,
            "/test",
            &[],
            None,
            Instant::now() + budget,
        ),
    )
    .await
    .map_err(|_| anyhow::anyhow!("request deadline"))?
}

#[tokio::test]
async fn concurrent_requests_share_one_connection_and_closed_connection_redials() {
    let server = endpoint(true).await;
    let client = endpoint(false).await;
    let session = Arc::new(session::ClientSession::default());
    let accepts = Arc::new(AtomicUsize::new(0));
    let requests = Arc::new(AtomicUsize::new(0));
    let handler = tokio::spawn({
        let server = server.clone();
        let accepts = accepts.clone();
        let requests = requests.clone();
        async move {
            while let Some(incoming) = server.accept().await {
                let connection = incoming.await.unwrap();
                accepts.fetch_add(1, Ordering::SeqCst);
                let requests = requests.clone();
                tokio::spawn(async move {
                    while let Ok((mut send, mut recv)) = connection.accept_bi().await {
                        recv.read_to_end(4096).await.unwrap();
                        requests.fetch_add(1, Ordering::SeqCst);
                        send.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok")
                            .await
                            .unwrap();
                        send.finish().unwrap();
                    }
                });
            }
        }
    });
    let (first, second) = tokio::join!(
        request(
            session.clone(),
            &client,
            &server,
            "GET",
            Duration::from_secs(2)
        ),
        request(
            session.clone(),
            &client,
            &server,
            "POST",
            Duration::from_secs(2)
        ),
    );
    for response in [first, second] {
        let mut response = response.unwrap();
        assert_eq!(response.body.read_chunk().await.unwrap().unwrap(), b"ok");
        assert!(response.body.read_chunk().await.unwrap().is_none());
    }
    assert_eq!(accepts.load(Ordering::SeqCst), 1);
    let addr = EndpointAddr::new(server.id()).with_ip_addr(server.bound_sockets()[0]);
    let connection = session.connection(&client, addr).await.unwrap();
    connection.close(0u32.into(), b"test disconnect");
    let mut response = request(session, &client, &server, "GET", Duration::from_secs(2))
        .await
        .unwrap();
    assert_eq!(response.body.read_chunk().await.unwrap().unwrap(), b"ok");
    assert_eq!(accepts.load(Ordering::SeqCst), 2);
    assert_eq!(requests.load(Ordering::SeqCst), 3);
    handler.abort();
    client.close().await;
    server.close().await;
}

#[tokio::test]
async fn stalled_mutation_response_times_out_without_replaying() {
    let server = endpoint(true).await;
    let client = endpoint(false).await;
    let received = Arc::new(AtomicUsize::new(0));
    let handler = tokio::spawn({
        let server = server.clone();
        let received = received.clone();
        async move {
            let connection = server.accept().await.unwrap().await.unwrap();
            let (_send, mut recv) = connection.accept_bi().await.unwrap();
            recv.read_to_end(4096).await.unwrap();
            received.fetch_add(1, Ordering::SeqCst);
            std::future::pending::<()>().await;
        }
    });
    let result = request(
        Arc::default(),
        &client,
        &server,
        "POST",
        Duration::from_millis(200),
    )
    .await;
    assert!(result.is_err());
    assert_eq!(received.load(Ordering::SeqCst), 1);
    handler.abort();
    client.close().await;
    server.close().await;
}

#[tokio::test]
async fn ordinary_body_keeps_total_deadline_but_sse_can_outlive_open_deadline() {
    for streaming in [false, true] {
        let server = endpoint(true).await;
        let client = endpoint(false).await;
        let handler = tokio::spawn({
            let server = server.clone();
            async move {
                let connection = server.accept().await.unwrap().await.unwrap();
                let (mut send, mut recv) = connection.accept_bi().await.unwrap();
                recv.read_to_end(4096).await.unwrap();
                let content_type = if streaming {
                    "text/event-stream"
                } else {
                    "text/plain"
                };
                send.write_all(
                    format!("HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\n\r\n").as_bytes(),
                )
                .await
                .unwrap();
                tokio::time::sleep(Duration::from_millis(250)).await;
                let _ = send.write_all(b"late body").await;
                let _ = send.finish();
                let _ = send.stopped().await;
            }
        });
        let mut response = request(
            Arc::default(),
            &client,
            &server,
            "GET",
            Duration::from_millis(150),
        )
        .await
        .unwrap();
        let body = response.body.read_chunk().await;
        if streaming {
            assert_eq!(body.unwrap().unwrap(), b"late body");
        } else {
            assert!(body.unwrap_err().to_string().contains("timed out"));
        }
        handler.abort();
        client.close().await;
        server.close().await;
    }
}
