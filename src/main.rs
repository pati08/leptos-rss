#![feature(let_chains)]

use cfg_if::cfg_if;
use rss_chat::socket::{ServerMessage, UserMessage};

cfg_if! {
    if #[cfg(feature = "ssr")] {
        use axum::{
            extract::{ws::WebSocket, WebSocketUpgrade},
            Extension,
            response::Response,
            routing::get,
            Router,
        };
        use futures::stream::{SplitSink, SplitStream};
        use std::time::Duration;
        use std::sync::{Arc, Mutex};

        mod ai;
        use ai::AiContext;

        mod commands;
    }
}

#[cfg(feature = "ssr")]
#[derive(Debug)]
enum ServerStateMessage {
    UserJoined { name: String },
    UserDisconnected { name: String },
    UserTyped { name: String },
    NewMessage { message: UserMessage },
    UserReadMessages { user: String, earliest: u32 },
}

#[cfg(feature = "ssr")]
#[derive(Clone)]
struct AppStateExt {
    state_broadcast_tx: tokio::sync::broadcast::Sender<ServerMessage>,
    state_tx: tokio::sync::mpsc::Sender<ServerStateMessage>,
    ai_context: Arc<AiContext>,
}

#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() {
    use std::collections::HashMap;

    use leptos::logging::log;
    use leptos::prelude::*;
    use leptos_axum::{generate_route_list, LeptosRoutes};
    use rss_chat::app::*;

    env_logger::init();

    const BROADCAST_CAPACITY: usize = 8;
    const STATE_CHANNEL_CAPACITY: usize = 8;

    const TYPING_TIME: Duration = Duration::from_millis(1500);

    let conf = get_configuration(None).unwrap();
    let addr = conf.leptos_options.site_addr;
    let leptos_options = conf.leptos_options;
    // Generate the list of routes in your Leptos App
    let routes = generate_route_list(App);

    let (state_broadcast_tx, _) =
        tokio::sync::broadcast::channel(BROADCAST_CAPACITY);
    let (state_tx, mut state_rx) =
        tokio::sync::mpsc::channel(STATE_CHANNEL_CAPACITY);

    let ai_context = AiContext::new(
        &std::env::var("GROQ_API_KEY").expect("No api key provided"),
    );

    let app_state = AppStateExt {
        state_broadcast_tx: state_broadcast_tx.clone(),
        state_tx,
        ai_context: Arc::new(ai_context),
    };

    let app_state_2 = app_state.clone();
    tokio::spawn(async move {
        let typing_counters: Arc<Mutex<Vec<(String, u64)>>> =
            Arc::new(Mutex::new(vec![]));
        let typing_users: Arc<Mutex<Vec<String>>> =
            Arc::new(Mutex::new(vec![]));
        let state_broadcast_tx = state_broadcast_tx.clone();
        let mut current_message_id = 0;

        let mut online_users: HashMap<String, u8> = HashMap::new();

        let send_msg = move |msg: ServerMessage| {
            if let Err(e) = state_broadcast_tx.send(msg) {
                log::error!("Error when sending state broadcast message:\n{e}");
            }
        };

        while let Some(msg) = state_rx.recv().await {
            match msg {
                ServerStateMessage::UserTyped { ref name } => {
                    let idx = {
                        let mut counters = typing_counters.lock().unwrap();
                        let mut typing_users = typing_users.lock().unwrap();

                        if !typing_users.contains(name) {
                            typing_users.push(name.clone());
                            send_msg(ServerMessage::UserTyping {
                                user: name.clone(),
                            })
                        }

                        if let Some(v) =
                            counters.iter_mut().find(|i| i.0 == *name)
                        {
                            v.1 += 1;
                            v.1
                        } else {
                            counters.push((name.to_string(), 0));
                            0
                        }
                    };
                    {
                        let name = name.clone();
                        let typing_users = typing_users.clone();
                        let typing_counters = typing_counters.clone();
                        let send_msg = send_msg.clone();
                        tokio::spawn(async move {
                            tokio::time::sleep(TYPING_TIME).await;
                            let counters = typing_counters.lock().unwrap();
                            let mut typing_users = typing_users.lock().unwrap();
                            if let Some(v) = counters
                                .iter()
                                .find(|i| i.0 == name)
                                .map(|v| v.1)
                                && v == idx
                                && let Some(typing_idx) =
                                    typing_users.iter().position(|i| *i == name)
                            {
                                typing_users.remove(typing_idx);
                                send_msg(ServerMessage::UserStoppedTyping {
                                    user: name,
                                });
                            }
                        });
                    }
                }
                ServerStateMessage::UserJoined { name } => {
                    *online_users.entry(name).or_default() += 1;

                    send_msg(ServerMessage::OnlineUsersUpdate {
                        users: online_users
                            .iter()
                            .filter(|(_n, i)| **i > 0)
                            .map(|(n, _i)| n.clone())
                            .collect(),
                    });
                }
                ServerStateMessage::NewMessage { mut message } => {
                    let original = message.clone();
                    message.id = current_message_id;
                    current_message_id += 1;
                    message.message = message
                        .message
                        .lines()
                        .map(|line| line.trim_end()) // Trim trailing spaces
                        .collect::<Vec<_>>() // Collect into a Vec
                        .join("  \n"); // Join with Markdown's line break syntax (two spaces + newline)

                    // Configure commonmark extensions
                    let mut comrak_options = comrak::Options::default();
                    macro_rules! enable_exts {
                        ($($x:ident),+ $(,)?) => {{
                            $(
                                comrak_options.extension.$x = true;
                            )*
                        }};
                    }
                    enable_exts! {
                        table,
                        strikethrough,
                        autolink,

                        // TODO: Implement CSS for this
                        alerts,

                        // TODO: Implement mathjax to SVG, then add math things

                        wikilinks_title_after_pipe,
                        underline,
                        subscript,
                        multiline_block_quotes,
                    };
                    comrak_options.render.hardbreaks = true;
                    comrak_options.render.escape = true;
                    comrak_options.render.ignore_empty_links = true;
                    comrak_options.parse.smart = true;

                    message.message = comrak::markdown_to_html(
                        &message.message,
                        &comrak_options,
                    );
                    log::debug!("Sending message:\n{message:?}");
                    send_msg(ServerMessage::MessageSent {
                        message: message.clone(),
                    });
                    let app_state = app_state.clone();
                    tokio::spawn(async move {
                        commands::react_to_message(original, app_state).await;
                    });
                }
                ServerStateMessage::UserDisconnected { name } => {
                    let ent = online_users.entry(name.clone()).or_default();
                    *ent = ent.saturating_sub(1);
                    if *ent == 0 {
                        online_users.remove(&name);
                    }
                    send_msg(ServerMessage::OnlineUsersUpdate {
                        users: online_users
                            .iter()
                            .filter(|(_n, i)| **i > 0)
                            .map(|(n, _i)| n.clone())
                            .collect(),
                    });
                }
                ServerStateMessage::UserReadMessages { user, earliest } => {
                    send_msg(ServerMessage::MessagesRead {
                        by_user: user,
                        earliest,
                    });
                }
            }
        }
    });

    let app = Router::new()
        .leptos_routes(&leptos_options, routes, {
            let leptos_options = leptos_options.clone();
            move || shell(leptos_options.clone())
        })
        .route("/api/ws", get(handler))
        .fallback(leptos_axum::file_and_error_handler(shell))
        .with_state(leptos_options)
        .layer(Extension(app_state_2));

    log!("listening on http://{}", &addr);
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app.into_make_service())
        .await
        .unwrap();
}

#[cfg(feature = "ssr")]
async fn handler(
    ws: WebSocketUpgrade,
    Extension(state): Extension<AppStateExt>,
) -> Response {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

#[cfg(feature = "ssr")]
async fn handle_socket(ws: WebSocket, state: AppStateExt) {
    use futures::StreamExt;

    let (sender, receiver) = ws.split();
    let read_task = tokio::spawn(handle_socket_read(receiver, state.clone()));
    let write_task = tokio::spawn(handle_socket_write(sender, state.clone()));

    let res = futures::join!(read_task, write_task);
    if let Err(e) = res.0 {
        log::error!("Error on websocket read task:\n{e}");
    }
    if let Err(e) = res.1 {
        log::error!("Error on websocket write task:\n{e}");
    }
}

#[cfg(feature = "ssr")]
async fn handle_socket_read(
    mut ws: SplitStream<WebSocket>,
    state: AppStateExt,
) {
    use codee::{binary::MsgpackSerdeCodec, HybridDecoder};
    use futures::StreamExt;
    use rss_chat::socket::*;
    use std::time::{Duration, Instant};

    const HEARTBEAT_MAX_INTERVAL: Duration = Duration::from_secs(5);

    let mut latest_heartbeat = Instant::now();

    let name = loop {
        let Some(Ok(msg)) = ws.next().await else {
            return;
        };
        if msg.to_text().is_ok_and(|v| v == "<Heartbeat>") {
            latest_heartbeat = Instant::now();
            continue;
        }
        let decoded: Result<ClientMessage, _> =
            MsgpackSerdeCodec::decode_bin(&msg.into_data());
        match decoded {
            Ok(ClientMessage::InitMessage { name }) => {
                break name;
            }
            _ => {
                log::error!("First message from client was not init message");
                return;
            }
        }
    };

    if state
        .state_tx
        .send(ServerStateMessage::UserJoined { name: name.clone() })
        .await
        .is_err()
    {
        log::error!("Channel not open or full when user joined");
        return;
    }

    while let Some(msg) = ws.next().await {
        let Ok(msg) = msg else {
            break;
        };
        if msg.to_text().is_ok_and(|v| v == "<Heartbeat>") {
            latest_heartbeat = Instant::now();
            continue;
        }
        if latest_heartbeat.elapsed() > HEARTBEAT_MAX_INTERVAL {
            log::info!("Client `{name}` timed out");
            break;
        }
        let data = msg.into_data();
        let decoded_result: Result<ClientMessage, _> =
            MsgpackSerdeCodec::decode_bin(&data);
        let Ok(decoded_result) = decoded_result else {
            break;
        };
        match decoded_result {
            ClientMessage::InitMessage { name } => {
                log::error!("Client {name} sent two init messages");
                break;
            }
            ClientMessage::Typed => {
                let _ = state
                    .state_tx
                    .send(ServerStateMessage::UserTyped { name: name.clone() })
                    .await;
            }
            ClientMessage::SendMessage { message } => {
                let _ = state
                    .state_tx
                    .send(ServerStateMessage::NewMessage { message })
                    .await;
            }
            ClientMessage::ReadMessages { earliest } => {
                let _ = state
                    .state_tx
                    .send(ServerStateMessage::UserReadMessages {
                        user: name.clone(),
                        earliest,
                    })
                    .await;
            }
        }
    }
    let _ = state
        .state_tx
        .send(ServerStateMessage::UserDisconnected { name })
        .await;
}

#[cfg(feature = "ssr")]
async fn handle_socket_write(
    mut ws: SplitSink<WebSocket, axum::extract::ws::Message>,
    state: AppStateExt,
) {
    use axum::extract::ws::Message;
    use codee::{binary::MsgpackSerdeCodec, HybridEncoder};
    use futures::SinkExt;

    let mut rx = state.state_broadcast_tx.subscribe();
    while let Ok(msg) = rx.recv().await {
        let encoded = match MsgpackSerdeCodec::encode_bin(&msg) {
            Ok(v) => v,
            Err(e) => {
                log::error!("Failed encoding message:\n{e}");
                continue;
            }
        };
        if ws.send(Message::Binary(encoded)).await.is_err() {
            return;
        }
    }
}

#[cfg(not(feature = "ssr"))]
pub fn main() {
    // no client-side main function
    // unless we want this to work with e.g., Trunk for pure client-side testing
    // see lib.rs for hydration function instead
}
