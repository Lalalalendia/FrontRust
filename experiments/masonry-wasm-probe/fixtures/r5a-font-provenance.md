# FrontRust R5a — Lato Cyrillic font provenance

- Source: **Lato Regular**, copyright (c) Łukasz Dziedzic, `fonts-lato` package (system upstream attribution: http://www.latofonts.com).
- Source file SHA-256: `6f6940be0835c3ddec9199e5fc42be4cbc61ebcfd58c623fdf719366253f1780` (`Lato-Regular.ttf`, 657,212 bytes).
- License: **SIL Open Font License 1.1**; redistributed license text: [OFL-Lato.txt](OFL-Lato.txt). Upstream reference: https://github.com/google/fonts/blob/main/ofl/lato/OFL.txt.
- Generated subset: `r5a-cyrillic-real.ttf`, 1,480 bytes, SHA-256 `4d31713156f2ae489c1a0d8a11cc5f06872b1a2ec04248f93831eb2c1b81f7c6`.
- Generated with `fontTools.subset.Subsetter`, text `Привет`; removed TrueType hint instructions and unrelated glyphs, removed unused layout features, retained cmap and glyph outlines/metrics, renamed font family to `FrontRust Demo Subset`, PostScript name `FrontRustDemoSubset-Regular`. This is a modified derivative, intentionally **not** using Lato reserved family name.
- Purpose: narrowly scoped CI fixture with **real distinct Cyrillic outlines**, not a complete font. The demo must not pretend to support general text or production font loading based on this subset.
- The separate R4e `r4e-layout-font.hex` remains for synthetic Unicode cursor/layout tests. It is not used as the visual scene font after R5a's real subset is installed.

## R5b Latin subset

- Second Latin source is the same Debian `fonts-lato` Lato Regular under SIL OFL 1.1, with the original upstream copyright attribution and license text unchanged in `OFL-Lato.txt`.
- `r5b-latin-real.ttf` is a **separately renamed and reduced** font for the four Latin characters in `Rust`; font family `FrontRust Latin Demo`, PostScript `FrontRustLatinDemo-Regular`.
- Size: **1560 bytes**. SHA-256: `4022495d87c257f960efeeda071a643c8b057c42e0e8d92649bb93e85d1c8dd2`. Git blob SHA-1: `87375daee921f636105b19a882f662a36faea0fe`.
- The original Lato Regular source checksum is recorded above. Reproduction: `fontTools.subset` over `/usr/share/fonts/truetype/lato/Lato-Regular.ttf`, glyph subset `Rust`, `hinting=False`, `layout_features=[]`, remove unused names, rename ID 1/2/3/4/6 and save as separate TTF. The committed byte hash is enforced in `tests/verify-masonry-font-fixtures.py`.
- This subset is NOT suitable for general Latin text, CJK, emoji, multilingual shaping, or browser font selection. R5b only demonstrates the explicit **whole-run** selection from two real upstream registered font families with complete `cmap` coverage and rejection of anything neither font fully covers.
