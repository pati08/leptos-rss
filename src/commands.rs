use crate::{ai, AppStateExt as AppState, ServerStateMessage};
use rss_chat::socket::UserMessage;
use thiserror::Error;

pub async fn react_to_message(message: UserMessage, state: AppState) {
    let user = message.sender.clone();
    let send_msg = move |msg: String, sender: String| async move {
        let _ = state
            .state_tx
            .send(ServerStateMessage::NewMessage {
                message: UserMessage {
                    send_time: chrono::Utc::now(),
                    sender,
                    message: msg,
                    id: 0,
                },
            })
            .await;
    };
    let send_msgc = send_msg.clone();
    let send_sysmsg = move |msg: String| send_msgc(msg, "System".to_string());
    match parse_commands(&message.message) {
        Some(Ok(command)) => match command {
            MessageCommand::AIQuery { bot, query } => {
                let response = state
                    .ai_context
                    .get_response(&query, &user, bot.as_deref())
                    .await;
                match response {
                    Ok(response) => {
                        send_msg(
                            response.response,
                            format!("{} (Bot)", response.bot_name),
                        )
                        .await;
                    }
                    Err(e) => {
                        send_sysmsg(format!("Bot could not respond:\n{e}"))
                            .await;
                    }
                }
            }
            MessageCommand::Help => {
                send_sysmsg(HELP_MESSAGE.to_string()).await;
            }
            MessageCommand::AIList => {
                let bots_list = state
                    .ai_context
                    .bots()
                    .await
                    .into_iter()
                    .map(|i| format!("- {}", i.name()))
                    .collect::<Vec<_>>()
                    .join("\n");
                send_sysmsg(format!("Bots online:\n{bots_list}")).await;
            }
            MessageCommand::AICreate { name, lang, config } => {
                let new_bot =
                    ai::Bot::new(name.clone(), user, Some(config), lang);
                state.ai_context.add_bot(new_bot).await;
                send_sysmsg(format!("Bot {name} created")).await;
            }
            MessageCommand::AIRemove { bot } => {
                if state.ai_context.remove_bot_by_name(bot).await.is_some() {
                    send_sysmsg("Bot removed".to_string()).await;
                } else {
                    send_sysmsg("No such bot exists".to_string()).await;
                }
            }
        },
        Some(Err(e)) => {
            send_sysmsg(format!("Invalid command:\n{e}")).await;
        }
        None => (),
    }
}

enum MessageCommand {
    AIQuery {
        bot: Option<String>,
        query: String,
    },
    AICreate {
        name: String,
        lang: Option<String>,
        config: String,
    },
    AIRemove {
        bot: String,
    },
    AIList,
    Help,
}

#[derive(Debug, Error)]
enum MessageParseError {
    #[error("Invalid command or command syntax entered")]
    InvalidCommand,
}

fn parse_commands(
    message: &str,
) -> Option<Result<MessageCommand, MessageParseError>> {
    let Some('%') = message.chars().next() else {
        return None;
    };
    let command_input = &message[1..];
    let Some(command) = command_input.split_whitespace().next() else {
        return Some(Err(MessageParseError::InvalidCommand));
    };
    if command == "ai" {
        Some(Ok(MessageCommand::AIQuery {
            bot: None,
            query: command_input[3..].to_string(),
        }))
    } else if command == "ask" && command_input.split_whitespace().count() > 2 {
        let bot_name = command_input.split_whitespace().nth(1).unwrap();
        let query = command_input
            .chars()
            .skip(3)
            .skip_while(|c| c.is_whitespace())
            .skip(bot_name.len())
            .skip_while(|c| c.is_whitespace())
            .collect();
        Some(Ok(MessageCommand::AIQuery {
            bot: Some(bot_name.to_string()),
            query,
        }))
    } else if command == "help" {
        Some(Ok(MessageCommand::Help))
    } else if command == "newbot"
        && command_input.split_whitespace().count() > 2
    {
        let name = command_input.split_whitespace().nth(1).unwrap();
        let lang_word = command_input.split_whitespace().nth(2).unwrap();
        let lang = lang_word.strip_prefix("lang=").map(|v| v.to_string());
        let customizations = command_input
            .chars()
            .skip(6)
            .skip_while(|c| c.is_whitespace())
            .skip(name.len())
            .skip_while(|c| c.is_whitespace());
        let customizations_final: String = if let Some(ref lang) = lang {
            customizations
                .skip(lang.len() + 5)
                .skip_while(|c| c.is_whitespace())
                .collect()
        } else {
            customizations.collect()
        };
        Some(Ok(MessageCommand::AICreate {
            name: name.to_string(),
            lang,
            config: customizations_final,
        }))
    } else if command == "listbots" {
        Some(Ok(MessageCommand::AIList))
    } else if command == "removebot" {
        Some(Ok(MessageCommand::AIRemove {
            bot: command_input[10..].to_string(),
        }))
    } else {
        Some(Err(MessageParseError::InvalidCommand))
    }
}

const HELP_MESSAGE: &str = "Valid commands:
- %ai <message> - ask a question to the default bot (greg)
- %ask <bot> <message> - ask a question to a bot by name
- %newbot <name> [lang=<language>] <instructions> - create a new
bot that follows custom instructions
- %listbots - list bots by name
- %removebot <bot> - remove a bot (you can only remove a bot you created)
- %help - show this message";
