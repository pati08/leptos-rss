use std::sync::Arc;

use codee::string::FromToStringCodec;
use js_sys::Object;
use leptos::{
    ev::SubmitEvent,
    html::{Input, Textarea},
    prelude::*,
};
use leptos_meta::{provide_meta_context, MetaTags, Stylesheet, Title};
use leptos_router::{
    components::{Route, Router, Routes},
    StaticSegment,
};
use leptos_use::{
    use_cookie, use_document_visibility, use_web_notification,
    use_websocket_with_options, UseWebSocketOptions,
};

use leptos::wasm_bindgen::prelude::*;
use leptos::wasm_bindgen::JsCast;
use leptos::web_sys::{CustomEvent, EventTarget};
use serde::{Deserialize, Serialize};

use crate::socket::UserMessageClient;

#[derive(Serialize, Deserialize, Clone)]
struct Settings {
    notification_settings: NotificationSetting,
}
#[derive(Serialize, Deserialize, Clone)]
enum NotificationSetting {
    AwayOnly,
    Always,
    Never,
}

pub fn shell(options: LeptosOptions) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1"/>
                <AutoReload options=options.clone() />
                <HydrationScripts options/>
                <MetaTags/>
                <script type="module" src="https://cdn.jsdelivr.net/npm/emoji-picker-element@^1/index.js"></script>
            </head>
            <body>
                <App/>
            </body>
        </html>
    }
}

#[component]
pub fn App() -> impl IntoView {
    // Provides context that manages stylesheets, titles, meta tags, etc.
    provide_meta_context();

    let style_url = if std::env::var("NIX_BUILD").is_ok_and(|v| v == "1") {
        let version = env!("CARGO_PKG_VERSION");
        format!("/pkg/rss-chat-{version}.css")
    } else {
        "/pkg/rss-chat.css".to_string()
    };

    view! {
        // injects a stylesheet into the document <head>
        // id=leptos means cargo-leptos will hot-reload this stylesheet
        <Stylesheet id="leptos" href=style_url/>

        // sets the document title
        <Title text="RSS Chat"/>

        // content for this welcome page
        <Router>
            <main>
                <Routes fallback=|| "Page not found.".into_view()>
                    <Route path=StaticSegment("") view=HomePage/>
                </Routes>
            </main>
        </Router>
    }
}

#[component]
fn HomePage() -> impl IntoView {
    use leptos_use::{use_permission, PermissionState};
    let (name, set_name) = use_cookie::<String, FromToStringCodec>("rss-name");

    let notification_permission = use_permission("notifications");
    let req_perm_node_ref: NodeRef<leptos::html::Dialog> = NodeRef::new();

    Effect::new(move || match notification_permission.get() {
        PermissionState::Prompt => {
            if let Some(v) = req_perm_node_ref.get() {
                let _ = v.show_modal();
            }
        }
        _ => {
            if let Some(v) = req_perm_node_ref.get() {
                v.close();
            }
        }
    });

    view! {
        <dialog class="p-8 rounded" node_ref=req_perm_node_ref>
            <button
                class="p-3 rounded shadow bg-gray-100 hover:bg-gray-200 active:bg-gray-400 transition"
                on:click=|_| {
                    let Ok(promise) = web_sys::Notification::request_permission() else {
                        return;
                    };
                    leptos::task::spawn_local(async move {
                        let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
                    });
                }
            >
                "Allow notifications"
            </button>
        </dialog>
        {move || match name.get() {
            Some(name) => view! {<Feed name=name/>}.into_any(),
            _ => view! {<SelectName set_name=set_name/>}.into_any(),
        }}
    }
}

/// Check whether a name is acceptable to use
fn validate_name(name: &str) -> bool {
    let name = name.trim().to_lowercase();
    !(name.ends_with("(bot)")
        | name.contains("system")
        | name.contains("admin")
        | (name.is_empty()))
}

#[component]
fn SelectName(set_name: WriteSignal<Option<String>>) -> impl IntoView {
    let input_node_ref: NodeRef<Input> = NodeRef::new();
    let on_submit = move |ev: SubmitEvent| {
        ev.prevent_default();
        let Some(name) = input_node_ref.get().map(|v| v.value()) else {
            return;
        };
        if validate_name(&name) {
            set_name.set(Some(name.trim().to_string()));
        }
    };
    view! {
        <main class="h-screen w-screen flex items-center">
            <div class="flex flex-col items-center w-full">
                <form class="bg-gray-100 rounded shadow p-6" on:submit=on_submit>
                    <input node_ref=input_node_ref placeholder="Enter your name..." class="p-2"/>
                    <button type="submit" class="p-2 ml-2 rounded bg-white hover:bg-gray-200 active:bg-gray-400 active:shadow transition-all">"Join"</button>
                </form>
            </div>
        </main>
    }
}

/// Renders the home page of your application.
#[component]
fn Feed(name: String) -> impl IntoView {
    use crate::socket::*;
    use leptos_use::UseWebSocketReturn;

    let settings =
        use_cookie::<Settings, codee::string::JsonSerdeCodec>("rss-settings");

    let UseWebSocketReturn {
        ready_state,
        message,
        send,
        ..
    } = use_websocket_with_options::<
        ClientMessage,
        ServerMessage,
        codee::binary::MsgpackSerdeCodec,
        Heartbeat,
        FromToStringCodec,
    >("/api/ws", UseWebSocketOptions::default().heartbeat(2000));

    let connection = Arc::new(ConnectionState::new(
        ready_state,
        message,
        send,
        name.clone(),
    ));

    let users = connection.users();
    let users = {
        let name = name.clone();
        Memo::new(move |_| {
            users
                .get()
                .into_iter()
                .filter(|i| i.0 != name)
                .collect::<Vec<_>>()
        })
    };

    let messages = connection.messages();

    let visibility = use_document_visibility();

    {
        let conn = connection.clone();
        let vis = if web_sys::VisibilityState::Visible != visibility.get() {
        } else {
        };
        Effect::new(move || {
            conn.update_visiblity(visibility.get());
        });
    }

    // Handle read receipts and notifications when receiving new messages
    {
        use leptos::web_sys::VisibilityState;
        let conn = connection.clone();
        Effect::new(move || {
            if let Some(ServerMessage::MessageSent { message }) =
                conn.message.get()
            {
                match visibility.get_untracked() {
                    VisibilityState::Hidden => {
                        (use_web_notification().show)(
                            leptos_use::ShowOptions::default()
                                .title(format!(
                                    "Message from {}",
                                    message.sender
                                ))
                                .body(message.get_short()),
                        );
                    }
                    VisibilityState::Visible => conn.read_messages(),
                    _ => (),
                }
            }
        });
    }
    // Read receipts when returning to the page
    {
        let conn = connection.clone();
        Effect::new(move |prev: Option<bool>| {
            if visibility.get() == leptos::web_sys::VisibilityState::Visible {
                if prev.is_some_and(|v| !v) {
                    conn.read_messages();
                }
                true
            } else {
                false
            }
        });
    }

    let (message_input, set_message_input) = signal(String::new());
    let message_node_ref: NodeRef<Textarea> = NodeRef::new();
    let (reply, set_reply) = signal::<Option<u32>>(None);

    let on_submit = {
        let connection = connection.clone();
        let name = name.clone();
        move || {
            connection.send_message(name.clone(), message_input.get(), reply());
            set_message_input.set(String::new());
            if let Some(message_el) = message_node_ref.get() {
                let _ = message_el.focus();
            }
            set_reply.set(None);
        }
    };

    let (users_info_open, set_users_info_open) = signal(true);
    let user_info_div_classes = move || {
        "absolute right-8 bottom-28 p-4 rounded shadow transition-all bg-white"
            .to_string()
            + if users_info_open.get() {
                " w-60 h-60"
            } else {
                " hover:bg-gray-200 active:bg-gray-400 hover:cursor-pointer"
            }
    };

    let (emoji_picker_open, set_emoji_picker_open) = signal(false);

    let reply_message = move || {
        reply.get().and_then(|r| {
            messages
                .get()
                .iter()
                .find(|m| m.get().message.id == r)
                .map(|v| v.get().clone())
        })
    };
    let reply_message = Signal::derive(reply_message);

    view! {
        {
            move || {
                let typing_users: Vec<_> = users
                    .get()
                    .into_iter()
                    .filter(|i| i.1)
                    .map(|i| view! {
                        <li>
                            {i.0} " is typing..."
                        </li>
                    })
                    .collect();
                if typing_users.is_empty() {
                    ().into_any()
                } else {
                    view! {
                        <div class="absolute top-4 right-4 p-4 rounded shadow bg-white">
                            <ul class="list-none">
                                {typing_users}
                            </ul>
                        </div>
                    }.into_any()
                }
            }
        }
        {
            let connection = connection.clone();
            view! {
                <EmojiPicker
                    callback=move |emoji: String| {
                        set_message_input.update(|v| v.push_str(&emoji));
                        connection.type_();
                        set_emoji_picker_open.set(false);
                    }
                    open=emoji_picker_open.into()
                />
            }
        }
        <div
            class=user_info_div_classes
            on:click=move |_ev| if !users_info_open.get() {
                set_users_info_open.set(true);
            }
        >
            {move || match users_info_open.get() {
                true => view!{
                    <UsersList users=Signal::derive(users) close=move || set_users_info_open.set(false) />
                }.into_any(),
                false => view!{"Users"}.into_any(),
            }}
        </div>
        <Messages messages=messages name=name.clone() set_reply=set_reply />
        <div class="fixed bottom-0 left-0 flex w-screen flex-col items-center justify-center">
            <ReplyInfo message=reply_message />
            <form
                // class="fixed bottom-0 left-0 flex w-screen flex-row items-center justify-center gap-2 bg-gray-200 p-3"
                class="flex w-screen flex-row items-center justify-center gap-2 bg-gray-200 p-3"
                on:submit={
                    let on_submit = on_submit.clone();
                    move |ev| {
                        ev.prevent_default();
                        on_submit();
                    }
                }
            >
                <div class="h-12 basis-2/3 rounded-sm bg-gray-50 shadow-xl ring-2 ring-gray-100 transition focus:outline-none focus:ring-gray-700 flex flex-row">
                    <textarea
                        placeholder="Your message..."
                        required
                        name="contents"
                        class="h-12 basis-2/3 rounded-sm bg-gray-50 px-3 ring-2 ring-gray-100 transition focus:outline-none focus:ring-gray-700 w-full flex-grow resize-none"
                        autocomplete="off"
                        spellcheck="false"
                        on:input:target=move |ev| {
                            set_message_input.set(ev.target().value());
                            connection.type_();
                        }
                        on:keydown={
                            let on_submit = on_submit.clone();
                            move |ev| {
                                if ev.key() == "Enter" && !ev.shift_key() {
                                    ev.prevent_default();
                                    on_submit();
                                }
                            }
                        }
                        prop:value=message_input
                        node_ref=message_node_ref
                    ></textarea>
                    <img src="emoji.png" class="max-h-full hover:cursor-pointer" on:click=move |_ev| set_emoji_picker_open.set(true) />
                </div>
                <button type="submit" class="h-12 basis-10 cursor-pointer rounded-sm bg-gray-50 px-3 font-bold shadow-xl ring-2 ring-gray-100 transition hover:bg-gray-800 hover:text-white hover:ring-0">"Send"</button>
            </form>
        </div>
    }
}

#[component]
fn ReplyInfo(message: Signal<Option<UserMessageClient>>) -> impl IntoView {
    move || {
        if let Some(UserMessageClient {
            message:
                crate::socket::UserMessage {
                    sender: sender_name,
                    message_short: reply_text,
                    ..
                },
            ..
        }) = message.get()
        {
            view! {
                <div class="bg-gray-100 pl-2 pr-20 py-1 rounded mb-2">
                    <div class="text-gray-700 text-sm">
                        <span class="italic">"Replying to "</span>
                        {sender_name}
                    </div>
                    <div inner_html=move || reply_text.clone()></div>
                </div>
            }
            .into_any()
        } else {
            ().into_any()
        }
    }
}

#[component]
fn Messages(
    messages: ReadSignal<Vec<ArcRwSignal<crate::socket::UserMessageClient>>>,
    name: String,
    set_reply: WriteSignal<Option<u32>>,
) -> impl IntoView {
    view! {
        <div>
            <For
                each=move || messages.get().into_iter().rev()
                key=|message| message.get().message.id
                let:message>
                <MessageDisplay
                    name=name.clone()
                    message=message.clone()
                    set_reply=set_reply
                    messages=messages/>
            </For>
        </div>
    }
}

#[component]
fn MessageDisplay(
    name: String,
    message: ArcRwSignal<UserMessageClient>,
    set_reply: WriteSignal<Option<u32>>,
    messages: ReadSignal<Vec<ArcRwSignal<UserMessageClient>>>,
) -> impl IntoView {
    let message_2 = message.clone();
    let reply_message = move || {
        message_2.get().message.reply_to.and_then(|r| {
            messages
                .get()
                .iter()
                .find(|m| m.get().message.id == r)
                .map(|v| v.get().clone())
        })
    };
    let reply_message = Signal::derive(reply_message);
    view! {
        <div class="hover:bg-gray-200 transition w-screen px-2 py-4">
            {let message = message.clone(); let name = name.clone(); move || {
                let read_by = message.get().read_by.into_iter().filter(|i| *i != name && *i != message.get().message.sender).collect::<Vec<_>>();
                if read_by.is_empty() {
                    ().into_any()
                } else {
                    view!{
                        <div>"Read by: " {read_by.join(", ")}</div>
                    }.into_any()
                }
            }}
            <div class="hover:bg-gray-200 transition flex flex-row w-full">
                <div class="grow pr-8">
                    <div class="font-bold text-gray-700">{ let message = message.clone(); move || message.get().message.sender }</div>
                    <ReplyInfo message=reply_message />
                    <div inner_html={ let message = message.clone(); move || message.get().message.message_html_safe }></div>
                </div>
                <div class="text-right text-gray-700 flex flex-row items-center shrink-0">
                    <div class="text-right w-full">
                        {
                            let message = message.clone();
                            move || format_datetime(message.get().message.send_time)
                        }
                    </div>
                </div>
                <div class="flex flex-col items-center justify-center ml-3">
                    <button
                        class="p-2 rounded shadow bg-white hover:bg-gray-200 active:bg-gray-400 transition hover:cursor-pointer"
                        on:click=
                            move |ev| {
                                ev.prevent_default();
                                set_reply.set(Some(message.get().message.id))
                            }
                    >
                        "Reply"
                    </button>
                </div>
            </div>
        </div>
    }
}

#[component]
fn UsersList(
    users: Signal<Vec<(String, bool, bool)>>,
    close: impl Fn() + 'static,
) -> impl IntoView {
    view! {
        <button
            class="p-1 rounded shadow absolute top-2 right-2"
            on:click=move |ev| {
                close();
                ev.stop_propagation();
            }
        >
            "❌"
        </button>
        {move || if users.get().is_empty() {
            view! {
                <p>"No other users online"</p>
            }.into_any()
        } else {
            view! {
                <ol class="list-decimal">
                    {users.get().into_iter().map(|i| {
                        let mut status = String::with_capacity(4);
                        if i.1 {
                            status.push_str(" ⌨️");
                        }
                        if i.2 {
                            status.push_str(" 👀");
                        }
                        view!{
                            <li class="ml-6">{i.0}{status}</li>
                        }
                    }).collect::<Vec<_>>()}
                </ol>
            }.into_any()
        }}
    }
}

#[component]
fn EmojiPicker(
    callback: impl Fn(String) + 'static + Clone + Send,
    open: Signal<bool>,
) -> impl IntoView {
    let dialog_ref = NodeRef::<leptos::html::Dialog>::new();
    let picker_ref = NodeRef::new();

    let on_mount = move || {
        let picker: Option<leptos::web_sys::HtmlElement> = picker_ref.get();
        if let Some(picker) = picker {
            let event_target: &EventTarget = picker.unchecked_ref();

            let callback = callback.clone();
            let closure =
                Closure::wrap(Box::new(move |event: leptos::web_sys::Event| {
                    if let Ok(custom_event) = event.dyn_into::<CustomEvent>() {
                        let obj = Object::from(custom_event.detail());
                        let arr = Object::values(&obj);
                        let emoji = arr.get(2);
                        if let Some(emoji) = emoji.as_string() {
                            callback(emoji);
                        }
                    }
                }) as Box<dyn FnMut(_)>);

            event_target
                .add_event_listener_with_callback(
                    "emoji-click",
                    closure.as_ref().unchecked_ref(),
                )
                .unwrap();

            closure.forget(); // Leak the closure (or store it in state)
        }
    };

    Effect::new(move || {
        if let Some(dialog) = dialog_ref.get() {
            if open.get() {
                let _ = dialog.show_modal();
            } else {
                dialog.close();
            }
        }
    });

    view! {
        <dialog node_ref=dialog_ref>
            <emoji-picker node_ref=picker_ref on_mount=on_mount></emoji-picker>
        </dialog>
    }
}

fn format_datetime(datetime: chrono::DateTime<chrono::Utc>) -> String {
    let now = chrono::Utc::now();
    let is_today = now.date_naive() == datetime.date_naive();
    let datetime: chrono::DateTime<chrono::Local> = datetime.into();
    if is_today {
        datetime.format("%I:%M %P").to_string()
    } else {
        datetime.format("%d %b, %Y - %I:%M %P").to_string()
    }
}
