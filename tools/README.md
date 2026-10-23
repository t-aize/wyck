# tools

Maintenance scripts. They are not part of the build: the files they write are committed, so a
normal checkout never needs to run them. They are kept to show where the assets come from.

| Script | Writes | When to run it |
|---|---|---|
| `make_brand_assets.py` | `crates/wyck/assets/app-icon.{png,ico,icns}` and `crates/wyck-ui-kit/assets/logo.png`, from the two images in `crates/wyck/assets/brand/` | A source image changed |
| `make_sounds.py` | the alert sounds in `crates/wyck-app/assets/sounds/` (synthesized, deterministic) | A sound must change |

Both need Python 3; `make_brand_assets.py` also needs Pillow (`pip install pillow`). Run them from the
repository root, then commit the results.
