Place Pumpkin-style Palm font text files (`*.txt`) in this directory to embed
them into the `tern-x4` firmware image at build time.

Expected naming is the same as the runtime `/fonts` loader, for example:

- `NFNT_9100.txt`
- `NFNT_9101.txt`

At compile time, `x4/build.rs` generates a table of embedded font sources and
`SdImageSource::load_prc_system_fonts()` loads them before SD-card fonts.

SD-card `/fonts` files are still supported and can be used as overrides during
development.
