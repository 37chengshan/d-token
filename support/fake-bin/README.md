# fake-bin

Fake RTK and Headroom binaries for no-touch tests.

These scripts are intentionally local test doubles. They read fixture files from `support/fixtures` and must not call the user's real RTK or Headroom installations.

Usage from repository root:

```bash
chmod +x support/fake-bin/rtk support/fake-bin/headroom
PATH="$PWD/support/fake-bin:$PATH" bash support/scripts/run_fake_binary_tests.sh
```
