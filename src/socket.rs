use leptos::prelude::*;
use leptos_use::core::ConnectionReadyState;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UserMessage {
    pub send_time: chrono::DateTime<chrono::Utc>,
    pub sender: String,
    pub message: String,
    pub reply_to: Option<u32>,
    pub id: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum VisibilityState {
    Hidden,
    Visible,
}

impl From<web_sys::VisibilityState> for VisibilityState {
    fn from(value: web_sys::VisibilityState) -> Self {
        match value {
            web_sys::VisibilityState::Visible => Self::Visible,
            _ => Self::Hidden,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum ClientMessage {
    InitMessage { name: String },
    SendMessage { message: UserMessage },
    Typed,
    ReadMessages { earliest: u32 },
    VisibilityUpdate(VisibilityState),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum ServerMessage {
    MessagesRead { by_user: String, earliest: u32 },
    MessageSent { message: UserMessage },
    UserTyping { user: String },
    UserStoppedTyping { user: String },
    // UserOnline { user: String },
    // UserOffline { user: String },
    OnlineUsersUpdate { users: Vec<String> },
    UserObserving { user: String },
    UserNotObserving { user: String },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UserMessageClient {
    pub message: UserMessage,
    /// List of users who have read the message
    pub read_by: Vec<String>,
}

#[derive(Default)]
pub struct Heartbeat;

impl std::fmt::Display for Heartbeat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<Heartbeat>")
    }
}

impl UserMessage {
    pub fn preview(&self) -> String {
        if self.message.len() <= 40 {
            self.message.clone()
        } else {
            format!("{}...", self.message.chars().take(37).collect::<String>())
        }
    }
}

pub struct ConnectionState<SendFn>
where
    SendFn: Fn(&ClientMessage) + Clone + Send + Sync + 'static,
{
    ready: Signal<ConnectionReadyState>,
    pub message: Signal<Option<ServerMessage>>,
    send: SendFn,
    messages: RwSignal<Vec<ArcRwSignal<UserMessageClient>>>,
    typing: RwSignal<Vec<String>>,
    online: RwSignal<Vec<String>>,
    observing: RwSignal<Vec<String>>,
}

impl<SendFn> ConnectionState<SendFn>
where
    SendFn: Fn(&ClientMessage) + Clone + Send + Sync + 'static,
{
    pub fn messages(&self) -> ReadSignal<Vec<ArcRwSignal<UserMessageClient>>> {
        self.messages.read_only()
    }
    pub fn typing(&self) -> ReadSignal<Vec<String>> {
        self.typing.read_only()
    }
    pub fn online(&self) -> ReadSignal<Vec<String>> {
        self.online.read_only()
    }
    pub fn users(&self) -> Signal<Vec<(String, bool, bool)>> {
        let typing = self.typing;
        let online = self.online;
        let observing = self.observing;
        Signal::derive(move || {
            online
                .get()
                .into_iter()
                .map(move |i| {
                    let is_typing = typing.get().contains(&i);
                    let is_observing = observing.get().contains(&i);
                    (i, is_typing, is_observing)
                })
                .collect()
        })
    }
    pub fn observing(&self) -> ReadSignal<Vec<String>> {
        self.observing.read_only()
    }
    pub fn new(
        ready: Signal<ConnectionReadyState>,
        last_message: Signal<Option<ServerMessage>>,
        send: SendFn,
        name: String,
    ) -> Self {
        let messages: RwSignal<Vec<ArcRwSignal<UserMessageClient>>> =
            RwSignal::new(vec![]);
        let typing = RwSignal::new(vec![]);
        let online: RwSignal<Vec<String>> = RwSignal::new(vec![]);
        let observing: RwSignal<Vec<String>> = RwSignal::new(vec![]);
        {
            Effect::new(move || {
                last_message.with(move |last_message| match last_message {
                    None => (),
                    Some(ServerMessage::MessageSent { message }) => {
                        let client_message = UserMessageClient {
                            message: message.clone(),
                            read_by: vec![],
                        };
                        messages.update(|history| {
                            history.push(ArcRwSignal::new(client_message))
                        })
                    }
                    Some(ServerMessage::UserTyping { user }) => {
                        typing.update(move |typing| {
                            if !typing.contains(user) {
                                typing.push(user.clone());
                            }
                        })
                    }
                    Some(ServerMessage::UserStoppedTyping { user }) => typing
                        .update(move |typing| {
                            *typing = typing
                                .iter()
                                .filter(|i| *i != user)
                                .cloned()
                                .collect();
                        }),
                    Some(ServerMessage::OnlineUsersUpdate { users }) => {
                        online.set(users.clone());
                    }
                    Some(ServerMessage::MessagesRead { by_user, earliest }) => {
                        messages.update(move |messages| {
                            for message in messages
                                .iter_mut()
                                .filter(|i| i.get().message.id >= *earliest)
                            {
                                if !message.get().read_by.contains(by_user) {
                                    message.update(|v| {
                                        v.read_by.push(by_user.clone())
                                    });
                                }
                            }
                        });
                    }
                    Some(ServerMessage::UserObserving { user }) => {
                        if !observing.get_untracked().contains(user) {
                            observing.update(|v| v.push(user.to_string()));
                        }
                    }
                    Some(ServerMessage::UserNotObserving { user }) => observing
                        .update(move |observing| {
                            *observing = observing
                                .iter()
                                .filter(|i| *i != user)
                                .cloned()
                                .collect();
                        }),
                })
            });
        }
        {
            let send = send.clone();
            Effect::new(move |prev: Option<bool>| match ready.get() {
                ConnectionReadyState::Open => {
                    if prev.is_none_or(|v| !v) {
                        send(&ClientMessage::InitMessage {
                            name: name.clone(),
                        });
                    }
                    true
                }
                _ => false,
            });
        }
        Self {
            ready,
            message: last_message,
            send,
            messages,
            typing,
            online,
            observing,
        }
    }
    pub fn send_message(
        &self,
        sender: String,
        message: String,
        reply_to: Option<u32>,
    ) {
        let send_time = chrono::Utc::now();
        let message = ClientMessage::SendMessage {
            message: UserMessage {
                id: 0,
                send_time,
                sender,
                message,
                reply_to,
            },
        };
        (self.send)(&message);
    }
    pub fn read_messages(&self) {
        let messages = self.messages.read();
        let Some(earliest) = messages.first() else {
            return;
        };
        let earliest = earliest.get_untracked().message.id;
        (self.send)(&ClientMessage::ReadMessages { earliest });
    }
    pub fn ready(&self) -> Signal<ConnectionReadyState> {
        self.ready
    }
    pub fn type_(&self) {
        (self.send)(&ClientMessage::Typed);
    }
    pub fn update_visiblity(&self, vis: impl Into<VisibilityState>) {
        (self.send)(&ClientMessage::VisibilityUpdate(vis.into()));
    }
}
