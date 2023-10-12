This is a game broker REST API for AI Wargame.

The general pattern for API access is

https://SERVER_ADDRESS/API_COMMAND?auth=AUTH_TOKEN&OTHER_OPTIONS

for example:
https://ai-wargame.csproject.org:10501/game/team23-vs-team21?auth=my-secret-code

Here is a summary of the API commands. When not specified, they are http GET commands and they require the "auth" parameter.

- /game<br>
Generates a game id. Each active game requires a unique id.

- /game/GAME_ID<br>
Returns a JSON object representing the last move played for GAME_ID.

- POST /game/GAME_ID<br>
Updates the internal state representing the last move played for GAME_ID.
This allows two programs with the same GAME_ID to play in sync with each other via the broker.

- /admin/state<br>
Shows a summary of the internal state associated with all game ids tracked by the broker.

- /admin/clear<br>
Clears all game ids.

To use the game broker with the Python template for Ai Wargame, you have to pass it as a command line option as show below:

- Player 1 runs: <br>
python ai_wargame.py --broker 'https://ai-wargame.csproject.org:10501/game/team23-vs-team21?auth=my-secret-code' --game_type attacker
- Player 2 runs: <br>
python ai_wargame.py --broker 'https://ai-wargame.csproject.org:10501/game/team23-vs-team21?auth=my-secret-code' --game_type defender

The broker is also capable of serving normal static web pages (without auth). This can be configured via a TOML config file.
ex: https://ai-wargame.csproject.org:10501/demo can serve a copy of the web demo for AI Wargame.

The authentication tokens for normal games and for admins are also stored in the config file.

An expiration date can be set for game state and a cleanup routine will remove all info for a game id after it has expired.

