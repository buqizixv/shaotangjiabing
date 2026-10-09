# OctoSense bridge for Daycast

English | [简体中文](README.zh-CN.md)

The bundle needs a host integration; it cannot obtain Matrix credentials or call arbitrary native services. `octosense-rinx.patch` contains the changes for the local OctoSense workspace that already imports Daycast as `os.weather-assistant`, including the system bundle UI. It is a patch against that existing Daycast integration, not a patch for a clean upstream OctoSense checkout.

The bridge validates Daycast's host-authenticated app ID, a foreground surface permission, the fixed rooms/send methods and exact room/text arguments. It queues `list_rooms`/`send_message` to the live Rinx module through the existing AI bus, preserves Rinx's native confirmation and encryption, forwards the actual outcome once, and cancels unanswered calls after two minutes. There is no login, token, raw HTTP send, generic tool dispatch or agent impersonation. Sending is opt-in in Daycast. Rinx must be open and signed in.

App Hub's `storage` capability still gates the UI request, and the host validates identity again; no new unrestricted capability is declared. The service extends only the existing Daycast storage implementation. `may_prompt=false` calls, including agent/service tool calls and home tiles, cannot use the bridge.

In the local OctoSense workspace, these commands were run:

```powershell
python -X utf8 tools/setup.py
cargo test --locked -p octosense-llm-service --test weather_advice
cargo test --locked -p octosense-shell --features app-hub,app-rinx daycast_rinx --lib
cargo build --locked --release -p octosense --bin octosense
```

The patch includes bridge tests. `weather-advice.rs` preserves the exercised Splash test suite; it belongs at `apps/weather-assistant/tests/advice.rs`, referenced by the local LLM-service test target. `system-manifest.json` and `system-listing.json` are the restamped system bundle metadata.

Applying this patch in another pre-integrated checkout and building on mobile are **unverified**. Native confirmation and actual delivery to a real family room still require a signed-in user's acceptance test. Standalone card-host can inspect the bundle UI but has no Rinx bridge. This feature checks weather only while Daycast is open and is not an OS background scheduler.
