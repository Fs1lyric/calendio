# Initial repository plan

Files: workspace and crate manifests, app identity, storage defaults, packaging,
UI theme, README, product roadmap and isolation tests.

Keep the existing tested Rust/SQLite calendar and booking implementation.
Give Calendio independent storage and executable/service names. Do not migrate
Calendi data or install/start services during repository setup. Use an off-white
calendar canvas, ink text, blue action accent, readable event blocks and larger
controls. Keep the full feature-parity backlog explicit in ROADMAP.md.

Verification: fail identity tests first, then pass; fail the new theme contrast
test before theme changes; run workspace tests, clippy and rustfmt; launch with
a disposable database and capture the actual app.
