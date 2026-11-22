use super::*;
fn fixture() -> (World, Entity) {
    let mut world = World::new();
    let parent = world.spawn((Layout::column(),));
    (world, parent)
}
fn ready(world: &mut World, key: Key) {
    apply(
        world,
        PreviewUpdate {
            key,
            result: Ok(Pixels {
                width: 20,
                height: 10,
                rgba: vec![255; 800],
            }),
        },
    );
}
#[test]
fn session_scoped_deduplicated_cached_and_late_replies_stay_scoped() {
    let (mut world, parent) = fixture();
    let a = mount(&mut world, parent, "a", "same", "one.png", false);
    let b = mount(&mut world, parent, "a", "same", "one.png", true);
    tick(&mut world);
    let commands = drain_commands(&mut world);
    assert_eq!(commands.len(), 1);
    let key = commands[0].clone();
    ready(&mut world, key.clone());
    tick(&mut world);
    assert!(world.get::<Preview>(a).unwrap().rendered);
    assert!(world.get::<Preview>(b).unwrap().rendered);
    assert_eq!(world.entities_with::<ImageView>().len(), 2);
    for _ in 0..5 {
        tick(&mut world);
    }
    assert!(drain_commands(&mut world).is_empty());
    despawn(&mut world, a);
    despawn(&mut world, b);
    let other = mount(&mut world, parent, "b", "same", "other.png", false);
    tick(&mut world);
    assert_eq!(drain_commands(&mut world).len(), 1);
    ready(&mut world, key);
    tick(&mut world);
    assert!(!world.get::<Preview>(other).unwrap().rendered);
    let cached = mount(&mut world, parent, "a", "same", "cached.png", false);
    tick(&mut world);
    assert!(world.get::<Preview>(cached).unwrap().rendered);
    assert!(drain_commands(&mut world).is_empty());
}
#[test]
fn error_is_readable_not_a_blank_image_or_repeated_request() {
    let (mut world, parent) = fixture();
    let root = mount(&mut world, parent, "s", "m", "bad.png", false);
    tick(&mut world);
    let key = drain_commands(&mut world).remove(0);
    apply(
        &mut world,
        PreviewUpdate {
            key,
            result: Err("Could not load image".into()),
        },
    );
    tick(&mut world);
    tick(&mut world);
    assert!(world.get::<Preview>(root).unwrap().rendered);
    assert!(world.entities_with::<ImageView>().is_empty());
    assert!(
        world
            .query::<Label>()
            .iter()
            .any(|(_, l)| l.text.contains("Preview unavailable"))
    );
    assert!(drain_commands(&mut world).is_empty());
}
#[test]
fn cache_is_bounded_and_aspect_fit_is_not_stretched() {
    let (mut world, parent) = fixture();
    for i in 0..40 {
        let root = mount(&mut world, parent, "s", &i.to_string(), "test.png", false);
        tick(&mut world);
        let key = drain_commands(&mut world).remove(0);
        ready(&mut world, key);
        tick(&mut world);
        despawn(&mut world, root);
    }
    assert!(world.resource::<Cache>().unwrap().entries.len() <= MAX_CACHE);
    let root = mount(&mut world, parent, "s", "shape", "shape.png", false);
    world.insert(
        root,
        Rect {
            w: 160.,
            ..Default::default()
        },
    );
    tick(&mut world);
    let key = drain_commands(&mut world).remove(0);
    ready(&mut world, key);
    tick(&mut world);
    let image = children_of(&world, root)
        .into_iter()
        .find(|e| world.has::<ImageView>(*e))
        .unwrap();
    let l = world.get::<Layout>(image).unwrap();
    assert_eq!(l.width, Size::Px(160.));
    assert_eq!(l.height, Size::Px(80.));
}
#[test]
fn decoder_rejects_garbage_and_downsamples_large_images() {
    assert!(decode(b"not an image").is_err());
    let source = image::RgbaImage::from_pixel(1600, 800, image::Rgba([255, 0, 0, 255]));
    let mut bytes = std::io::Cursor::new(Vec::new());
    source
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    let pixels = decode(bytes.get_ref()).unwrap();
    assert_eq!((pixels.width, pixels.height), (800, 400));
    assert_eq!(pixels.rgba.len(), 800 * 400 * 4);
    let source = image::RgbaImage::new(8193, 1);
    let mut bytes = std::io::Cursor::new(Vec::new());
    source
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    assert!(decode(bytes.get_ref()).is_err());
}
#[test]
fn opaque_media_path_cannot_escape_origin_and_session_is_required() {
    assert_eq!(
        path(&Key {
            session: "s&x=bad".into(),
            media: "../../foo?x#bar".into(),
            detailed: false
        }),
        "/v1/media/..%2F..%2Ffoo%3Fx%23bar?session_id=s%26x%3Dbad"
    );
}
#[tokio::test]
async fn binary_transport_uses_auth_and_bounds_even_chunked_responses() {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        for chunked in [false, true] {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                .unwrap();
            let mut raw = vec![];
            let mut buf = [0; 1024];
            loop {
                let n = socket.read(&mut buf).unwrap();
                raw.extend_from_slice(&buf[..n]);
                if raw.windows(4).any(|b| b == b"\r\n\r\n") {
                    break;
                }
            }
            let request = String::from_utf8(raw).unwrap().to_lowercase();
            assert!(request.contains("authorization: bearer fixture"));
            assert!(request.starts_with("get /v1/media/m?session_id=s "));
            let response = if chunked {
                "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n6\r\n123456\r\n0\r\n\r\n"
            } else {
                "HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\npng!"
            };
            socket.write_all(response.as_bytes()).unwrap();
        }
    });
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::AUTHORIZATION,
        reqwest::header::HeaderValue::from_static("Bearer fixture"),
    );
    let http = reqwest::Client::builder()
        .no_proxy()
        .default_headers(headers)
        .build()
        .unwrap();
    let transport = medousa_sdk::HttpTransport::with_client(http);
    use medousa_sdk::transport::Transport;
    assert_eq!(
        transport
            .get_bytes(&origin, "/v1/media/m?session_id=s", 4)
            .await
            .unwrap(),
        b"png!"
    );
    assert!(
        transport
            .get_bytes(&origin, "/v1/media/m?session_id=s", 4)
            .await
            .is_err()
    );
    server.join().unwrap();
}

#[tokio::test]
async fn live_history_stream_keeps_partial_utf8_and_crlf_until_complete() {
    use futures_util::{StreamExt, stream};
    let payload = "data: {\"text\":\"quiet — image 🖼\"}\r\n\r\n: keepalive\n\ndata: [DONE]\n";
    // Every byte boundary, including within multi-byte Unicode and data: prefix.
    let chunks = payload.bytes().map(|b| Ok(bytes::Bytes::from(vec![b])));
    let lines = medousa_sdk::streaming::SseLineStream::new(stream::iter(chunks))
        .collect::<Vec<_>>()
        .await;
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].as_ref().unwrap(), "{\"text\":\"quiet — image 🖼\"}");
}

#[test]
fn expanding_uses_scoped_detail_request_and_escape_restores_focus() {
    let (mut world, parent) = fixture();
    world.insert_resource(Viewport::new(1200., 850.));
    world.insert_resource(ActionIntents::default());
    world.insert_resource(Focus::default());
    let root = mount(&mut world, parent, "s", "m", "mock.png", false);
    tick(&mut world);
    let key = drain_commands(&mut world).remove(0);
    ready(&mut world, key);
    tick(&mut world);
    let image = world.entities_with::<OpenImage>()[0];
    assert_eq!(try_activate(&mut world, image), ActivationResult::Accepted);
    tick(&mut world);
    let keys = drain_commands(&mut world);
    assert_eq!(keys.len(), 1);
    assert!(keys[0].detailed);
    assert_eq!(keys[0].session, "s");
    assert_eq!(path(&keys[0]), "/v1/media/m?session_id=s");
    let viewer = world
        .resource::<Option<Viewer>>()
        .unwrap()
        .as_ref()
        .unwrap();
    assert!(world.has::<Modal>(viewer.root));
    assert_eq!(viewer.anchor, root);
    world.insert_resource(Input {
        keys: vec![KeyEvent::named(NamedKey::Escape, Modifiers::default())],
        ..Default::default()
    });
    tick(&mut world);
    assert!(world.resource::<Option<Viewer>>().unwrap().is_none());
    assert_eq!(world.resource::<Focus>().unwrap().entity, Some(image));
    assert!(world.is_alive(root));
    assert!(world.resource::<Input>().unwrap().keys.is_empty());
    open_image(&mut world, image);
    despawn(&mut world, root);
    tick(&mut world);
    assert!(world.resource::<Option<Viewer>>().unwrap().is_none());
}

#[test]
fn detail_decode_keeps_readable_resolution_separate_from_thumbnail() {
    let source = image::RgbaImage::from_pixel(1600, 1000, image::Rgba([10, 20, 30, 255]));
    let mut bytes = std::io::Cursor::new(Vec::new());
    source
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    let thumb = decode_at(bytes.get_ref(), false).unwrap();
    let detail = decode_at(bytes.get_ref(), true).unwrap();
    assert_eq!((thumb.width, thumb.height), (800, 500));
    assert_eq!((detail.width, detail.height), (1600, 1000));
}

#[test]
fn zoom_controls_fit_bounds_close_and_detail_cache_are_bounded() {
    let (mut world, parent) = fixture();
    world.insert_resource(ActionIntents::default());
    world.insert_resource(Viewport::new(900., 650.));
    for i in 0..5 {
        let root = mount(&mut world, parent, "s", &format!("m{i}"), "mock.png", false);
        tick(&mut world);
        for key in drain_commands(&mut world) {
            ready(&mut world, key);
        }
        tick(&mut world);
        let image = children_of(&world, root)
            .into_iter()
            .find(|e| world.has::<OpenImage>(*e))
            .unwrap();
        open_image(&mut world, image);
        tick(&mut world);
        for key in drain_commands(&mut world) {
            ready(&mut world, key);
        }
        tick(&mut world);
        let controls = [
            "Zoom out",
            "Zoom in",
            "Fit image to window",
            "Close image preview",
        ]
        .map(|name| {
            world
                .query::<Semantics>()
                .iter()
                .find(|(e, s)| world.has::<Button>(*e) && s.name.as_deref() == Some(name))
                .map(|(e, _)| *e)
                .unwrap()
        });
        for _ in 0..10 {
            assert_eq!(
                try_activate(&mut world, controls[1]),
                ActivationResult::Accepted
            );
        }
        assert_eq!(
            world
                .resource::<Option<Viewer>>()
                .unwrap()
                .as_ref()
                .unwrap()
                .zoom,
            6.
        );
        assert_eq!(
            try_activate(&mut world, controls[2]),
            ActivationResult::Accepted
        );
        assert_eq!(
            world
                .resource::<Option<Viewer>>()
                .unwrap()
                .as_ref()
                .unwrap()
                .zoom,
            1.
        );
        assert_eq!(
            try_activate(&mut world, controls[0]),
            ActivationResult::Accepted
        );
        assert_eq!(
            world
                .resource::<Option<Viewer>>()
                .unwrap()
                .as_ref()
                .unwrap()
                .zoom,
            1.
        );
        assert_eq!(
            try_activate(&mut world, controls[3]),
            ActivationResult::Accepted
        );
        assert!(world.resource::<Option<Viewer>>().unwrap().is_none());
        assert!(
            world
                .resource::<Cache>()
                .unwrap()
                .entries
                .keys()
                .filter(|k| k.detailed)
                .count()
                <= 2
        );
        despawn(&mut world, root);
    }
}

#[test]
fn overflow_opens_first_hidden_image_and_browses_whole_set() {
    let (mut world, parent) = fixture();
    let images: Vec<_> = (0..5)
        .map(|i| (format!("m{i}"), format!("{i}.png")))
        .collect();
    let row = gallery(&mut world, parent, "s", &images);
    tick(&mut world);
    assert_eq!(drain_commands(&mut world).len(), 2);
    let more = children_of(&world, row)
        .into_iter()
        .find(|e| world.has::<OpenImage>(*e))
        .unwrap();
    open_image(&mut world, more);
    for index in [2, 3, 4, 0, 1] {
        let v = world
            .resource::<Option<Viewer>>()
            .unwrap()
            .as_ref()
            .unwrap();
        assert_eq!(v.source.key.media, images[index].0);
        assert_eq!(v.focus, more);
        let mut input = Input::default();
        input
            .keys
            .push(KeyEvent::named(NamedKey::ArrowRight, Modifiers::default()));
        world.insert_resource(input);
        sync_viewer(&mut world);
    }
    close_viewer(&mut world);
    assert!(world.is_alive(row));
}
