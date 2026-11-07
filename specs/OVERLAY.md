1. Overlay presentation requirement :
Line 1 (MANUAL) : | [F9] CYCLE MODE | [F10] -5 FPS | | [F11] +5 FPS | [F12] OSD |
Line 1 (AUTO) : | [F9] CYCLE MODE | [F10] -1 % | | [F11] +1 % | [F12] OSD |
Line 2 : | MODE: MANUAL/AUTO (index)/OFF | CAP: xx FPS| HYS: xx % |
Line 3 : | xx FPS | 10% LOW: xx FPS | JIT: xx MS |
Line 4 : | GPU: xx °C | VRM: xx °C | Load: xx % | PPT: xx W |
Line 5 : | CPU: xx °C | VRM: xx °C | Load: xx % | RPM: xx RPM |
Line 6 (si `[overlay].jitter_graph_startup = 1`) : JITTER 0-50ms

Le graphique de jitter est désactivé par défaut (`jitter_graph_startup = 0`).
Lorsqu'il est désactivé, la ligne et son historique ne sont pas rendus/calculés.

2. Columns must be aligned accross all lines:
  "| [F10]", "| CAP:", "| 10% LOW", " | VRM:" must be vertically aligned
  "| [F11] +1 %", "| HYS: xx %", "| JIT:", and "| Load: xx %" must be vertically aligned
  "| [F12]", "| PPT:" and "| RPM:" must be vertically aligned


3. Highlighting :
- Any change of cap must briefly highlight "CAP: xx FPS" (1 sec)
- Any change of active algorithm hysteresis must briefly highlight "HYS: xx %" (1 sec)
- Any press of key must highlight the corresponding key in the first row (1 sec)

4. Background
- Background opacity is configurable through `[overlay].background_opacity` in `framecap.toml`.
- `0.0` is fully transparent, `1.0` is opaque, and the built-in default is `0.25`.

5. Refresh rate
- The overlay is redrawn at every frame presentation
- The overlay's metrics are refreshed every `[overlay].refresh_rate` presentations (default is 10)
- Sensor readings are averaged over `[overlay].gpu_smoothing_window_ms`, independently per sensor; unavailable readings clear only that sensor's smoothing history.
