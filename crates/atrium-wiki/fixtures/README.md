# Mneme's example responses

Copied from `tests/fixtures/api/` in Mneme at commit `9e7bfeed00eb4dade2fb7023791a8fbd1b85cf1f`. Mneme's own tests check every real response against these shapes, both ways, so Atrium's stub serves them as they are. `docs/api.md` in Mneme describes each route. `report.json` is the body an autosave service posts to `/git/status`; the test against a real Mneme posts it.

Copy them again when Mneme's API changes, and note the new commit here.

`questions.json` is Atrium's own, in the shape agreed for `GET /v1/questions`, which Mneme does not serve yet. Replace it with Mneme's example once it does.
