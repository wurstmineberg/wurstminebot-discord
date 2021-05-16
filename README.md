**wurstminebot** is [Wurstmineberg](https://wurstmineberg.de/)'s [Discord](https://discord.com/) bot. This project is the successor to [the original wurstminebot](https://github.com/wurstmineberg/wurstminebot) from the [IRC](https://en.wikipedia.org/wiki/Internet_Relay_Chat) era and to [slackminebot](https://github.com/wurstmineberg/slackminebot) from the [Slack](https://slack.com/) era.

# Features

Currently, the bot has the following features:

* **Commands:** There are a few commands that can be used in Discord for things like managing self-assignable roles or (for admins) updating Minecraft. For an overview, see the pinned message in the Discord channel #bot-spam.
* **Chatsync:** Chat messages written in Minecraft's in-game chat are cross-posted to that world's Discord text channel, and vice versa.
* **IPC interface:** A set of [commands](https://github.com/wurstmineberg/wurstminebot-discord/blob/main/crate/wurstminebot/src/ipc.rs) that can be run by other processes on the server, such as [the website](https://github.com/wurstmineberg/wurstmineberg.de), to make the bot do stuff. For example, this is used to post a message in the Discord channel #wiki when [our wiki](https://wurstmineberg.de/wiki) is edited.
* ~~**Twitch chat integration:** For members who have signed in with a [Twitch](https://twitch.tv/) account on our website, their Twitch chat is displayed in-game for convenience.~~ This is currently broken due to [museun/twitchchat#237](https://github.com/museun/twitchchat/issues/237).
* **Voice state exporter:** Writes information about who is currently connected to voice channels to disk for consumption by [the API](https://wurstmineberg.de/api).

# Installation

1. [Install Rust](https://www.rust-lang.org/tools/install)
2. `sudo systemctl enable --now assets/wurstminebot.service`
