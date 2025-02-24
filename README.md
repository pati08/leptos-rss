A simple chat feed with whatever stuff I felt like adding
# Features
- Typing indicators
- Commands with messages starting with "!"
    - create, manage, and talk to bots
    - get help
- Markdown formatting of messages
- Notifications
- Read receipts

# Setup and running
## Building
Build with nix:
- `nix build` - build a docker image for x86_64

Build and run with cargo-leptos:
- `cargo leptos serve --release`

## Running

### Ports
The server listens on port 3000 unless another is selected through the environment variable.

## Environment
There are environment variables with default values used to control behavior. The only required one is `GROQ_API_KEY`.

The following optional environment variables are also supported:

Name|Value|Description
--- | --- | ----------
`LEPTOS_SITE_ADDR` | `unsigned_int` | address to listen on
`AI_MAX_HISTORY_CHARS` | `unsigned_int` | maximum number of characters before cutting off messages in AI context
`BOT_SAVE_PATH` | `path` | path to save and read bot data from
