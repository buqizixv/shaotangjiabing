# Daycast Travel Plan: evidence-based readiness (0.6.0)

On entry, Daycast inventories the request, destination forecast, wardrobe, weather preferences, and both Daycast and OctoSense system-calendar events. The host determines readiness from verifiable conversation evidence and available data; a model cannot decide readiness by claiming confidence alone. Missing details appear as a small set of choices. “Some weekend” prompts for dates instead of silently choosing the next weekend. The user can pause at any time and resume only by choice.

When ready, the planner produces a day-by-day itinerary, outfits mapped to wardrobe items, a packing list, rain/cold-weather alternatives driven by weather rules, and a visual flow diagram. Destination, date range, weather coverage, wardrobe references, and calendar conflicts are checked again before review. The user inspects the complete plan and confirms before writing. Each calendar gets a batch conflict check; retries for the same plan version are idempotent. Daycast events retain a full plan snapshot and can reopen the flow diagram.

Weather rechecks run only while Daycast is open (periodic checks are user-enabled). A material change suggests a review; the user confirms before any replacement. **Flight changes are not monitored.** Preference feedback is applied only after the user explicitly saves it.

## Native UI evidence

Screenshots use isolated Windows QA data. Itinerary content comes from test fixtures and does not establish real-model quality, live-weather accuracy, transit data, or business hours.

![Missing details and date choices](screenshots/asking.png)

![User-paused plan](screenshots/paused.png)

![Ready itinerary draft](screenshots/ready.png)

![Daily outfits and packing list](screenshots/package.png)

![Weather-branch itinerary flow](screenshots/flow.png)

![Complete itinerary review before sync](screenshots/review.png)

![Saved itinerary in Daycast calendar](screenshots/calendar.png)

![Saved plan details opened from calendar](screenshots/saved-details.png)

## Host interfaces

- `daycast.plan.inventory` reads both Daycast and system-calendar events for conflict analysis.
- `daycast.plan.assess` evaluates user messages, explicit answers, and available data; model-authored confidence claims do not satisfy evidence requirements.
- `daycast.plan.validate` rechecks destination, dates, forecast coverage, wardrobe, and event times without writing.
- `daycast.plan.sync` requires user confirmation and batch-syncs with conflict protection to each calendar.
- `daycast.plan.recheck` compares the itinerary weather baseline with current forecast data and only suggests whether a review is warranted.

The system-calendar bridge lives in `host-integration/calendar-service/`; Daycast receives no general calendar-write permission. The two calendar files are replaced atomically separately, so the overall operation is not a cross-store transaction. If the first succeeds but an external concurrent change causes the second to reject, the user may retry from the UI; the plan ID is idempotent.

## Verification and limits

- `octosense-daycast-service`: 21 passed; one QWeather network test requiring a real key was ignored by configuration.
- `weather_advice`: 37 passed, covering app pages, state, geocoding candidates, and place disambiguation.
- Isolated native QA covered ambiguous-weekend questions, pause/resume, eight sample events written to both isolated calendars, plan attachments, flow diagrams, and explicit preference saving.
- Real model-provider output quality was not verified. Forecast/geocoding use Open-Meteo; live-service availability and long-range forecast accuracy are outside this verification.
- Weather rechecks do not run while the app is closed. No flight data source or flight monitoring is included.
- The OctoSense release executable was not rebuilt after the final small source changes: the default Cargo sandbox encountered Windows `PermissionDenied` in an external Rust Git checkout, and the elevated build request did not pass the automatic usage-limit review. The previously verified release binary does not contain the final source changes.
- Android/iOS devices and external calendar providers were not tested. No GitHub Release was created.

For the host implementation and integration notes, see [`host-integration`](../../host-integration/README.md). See [`README.zh-CN.md`](README.zh-CN.md) for Chinese documentation.
