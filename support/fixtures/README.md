# fixtures

Fixture files used by no-touch tests.

These files simulate RTK and Headroom outputs so d-token can test parsers, API responses, dashboards, event ledger, and layered savings attribution without touching the user's real local setup.

Rules:

1. Fixtures must not contain real prompts, real code, real command output, real provider keys, or real user paths.
2. Fixture mode must not execute real `rtk` or `headroom` binaries.
3. Fixture mode should be the default mode for GUI demos and parser tests.
