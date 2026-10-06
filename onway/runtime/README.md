# OctoSense integration

Base: OctoSense-org/OctoSense at `4a541777298eb4f85d9ac8ec2b83fab9e12ce909`.

`octosense-onway.patch` contains the Onway shell integration and Windows location patch registration. It adds the Python worker supervisor, dynamic card lifecycle and main-window hide/restore behavior. Apply it with `git apply /path/to/onway/runtime/octosense-onway.patch` in the pinned checkout. Copy `makepad-windows-location.patch` into the runtime's `tools/runtime-patches/` before running its dependency setup and Windows release build instructions.

Patch applicability against the exact base was checked. A clean build on another machine is unverified; the running local patched desktop was verified. No full framework, machine-specific Cargo configuration, binary or signing key is included.

Use the installer's own admitted Onway bundle, catalog and trust anchor. ONWAY_APP_DIR points at this project and ONWAY_PYTHON at Python 3.11+. launch.ps1 accepts HostExe, DataDir and Anchor and sets these environment variables.
