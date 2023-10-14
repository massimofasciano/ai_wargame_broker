This is a game broker REST API for AI Wargame.

The general pattern for API access is

https://USER:PASSWORD@SERVER_ADDRESS/API_COMMAND

for example:
https://USER:PASSWORD@ai-wargame.csproject.org:10501/game/team23-vs-team21

The USER and PASSWORD are managed using basic auth. This works on all browsers.
https://SERVER_ADDRESS/API_COMMAND
will work if USER and PASSWORD are sent via the request header "Authorization".

Some clients (Python, Rust, most browsers) also support the https://USER:PASSWORD@... syntax.

If your client does not support it and sending auth via the headers is impossible, you can use the legacy query params "username" and "password".
https://SERVER_ADDRESS/API_COMMAND?username=USER&password=PASSWORD

Here is a summary of the API commands. When not specified, they are http GET commands and they require a user and a password.

- /game<br>
Generates a game id. Each active game requires a unique id.

- /game/GAME_ID<br>
Returns a JSON object representing the last move played for GAME_ID.

- POST /game/GAME_ID<br>
Updates the internal state representing the last move played for GAME_ID.
This allows two programs with the same GAME_ID to play in sync with each other via the broker.

- /admin/state?refresh=N<br>
Shows a summary of the internal state associated with all game ids tracked by the broker.
If specified, refresh=N will force a refresh of the page every N seconds.

- DELETE /admin/clear<br>
Clears all game ids.

To use the game broker with the Python template for AI Wargame, you have to pass it as a command line option as show below:

- Player 1 runs: <br>
python ai_wargame.py --broker 'https://USER:PASSWORD@ai-wargame.csproject.org:10501/game/team23-vs-team21' --game_type attacker
- Player 2 runs: <br>
python ai_wargame.py --broker 'https://USER:PASSWORD@ai-wargame.csproject.org:10501/game/team23-vs-team21' --game_type defender

The broker is also capable of serving normal static web pages (without auth). This can be configured via a TOML config file.
ex: https://ai-wargame.csproject.org:10501/demo can serve a copy of the web demo for AI Wargame.

The users are also stored in the config file.

An expiration date can be set for game state and a cleanup routine will remove all info for a game id after it has expired.

If you don't want to include the username/password in the request URL (...USER:PASSWORD@...), you can place it in a netrc file and Python will use that automatically.

Format of the file:
```
machine ai-wargame.csproject.org
login USER
password PASSWORD
```
On Unix-like systems (Linux, MacOS), the netrc file should be placed in your HOME directory and named ``.netrc``.
On Windows, it should be at ``C:\USERS\your_user_name\_netrc``

Example config file (``ai_wargame.broker.toml``):
```toml
[network]
ip = "0.0.0.0"
# ip = "[::]"
port = 8000

[tls]
cert = "cert.pem"
key = "privkey.pem"
#cert = "/etc/letsencrypt/live/DOMAIN/fullchain.pem"
#key = "/etc/letsencrypt/live/DOMAIN/privkey.pem"
# enable http, https or both
enabled = "both"

[general]
internal = "/demo/internal"
expires = 600
cleanup = 60
# role for unauthenticated users (defaults to guest)
# unauthenticated = "user"

# array of static file trees

[[statics]]
uri = "/demo/live"
path = "../ai_wargame_web"

# array of users

[[users]]
name = "root"
password = "ag3nt"
role = "admin"

[[users]]
name = "student"
password = "s3cr3t"
role = "user"
```
