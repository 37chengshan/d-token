# d-token Docs

Start here:

1. `../README.md` — GitHub project introduction and quick orientation.
2. `PRD.md` — single living product source of truth.
3. `TECH_STACK.md` — desktop and runtime technology decision.
4. `ARCHITECTURE.md` — runtime architecture.
5. `SECURITY_BOUNDARIES.md` — local-first safety boundaries.
6. `FINAL_REVIEW.md` — final multidimensional review.
7. `M1_STATUS.md` — M1 implementation status.
8. `M1_REVIEW.md` — M1 implementation review.

Research and reviews:

9. `reviews/PRD_MULTIDIMENSIONAL_GAP_REVIEW_2026-07-01.md` — PRD gap review.
10. `reviews/RESEARCH_REPORTS_MULTIDIMENSIONAL_REVIEW_2026-07-01.md` — research reports review and expansion plan.
11. `research/REPORT_01_RTK_HEADROOM_INTERNALS.md` — RTK and Headroom internals, safety boundaries, event attribution.
12. `research/REPORT_02_DESKTOP_INTEGRATION_ONE_CLICK_APP.md` — desktop integration, installation transaction, UX states, compatibility matrix.
13. `research/REPORT_03_FEATURES_AND_FIXES_IMPLEMENTATION.md` — implementation blueprint, crates, APIs, CI gates, phased roadmap.

Second-pass additions now reflected in the PRD:

- Root `README.md` is part of the accepted repository structure.
- `DtokenEvent` / event ledger is a first-class model.
- `InstallationTransaction` is a first-class model.
- `CompatibilityMatrix` / `CompatibilityCase` is a first-class model.
- `support/scripts/check_structure.py` now requires root `README.md` instead of forbidding it.
