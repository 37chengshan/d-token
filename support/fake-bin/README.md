# fake-bin

Fake RTK and Headroom binaries for no-touch tests.

These scripts are intentionally local test doubles. They read fixture files from `support/fixtures` and must not call the user's real RTK or Headroom installations.

`managed-rtk-command` / `managed-rtk-command.cmd` are the managed-RTK wrapped-command fixtures: they emit a bounded repeated/stdout and `SECRET=must-not-persist` stderr marker and exit with the requested mode so the wrapper's exit-propagation contract is testable on macOS and Windows.

Usage from repository root:

```bash
chmod +x support/fake-bin/rtk support/fake-bin/headroom
PATH="$PWD/support/fake-bin:$PATH" bash support/scripts/run_fake_binary_tests.sh
```
