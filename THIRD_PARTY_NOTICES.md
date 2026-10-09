# Third-party notices

All of fuckMail's own code is released under the [MIT License](LICENSE). The app builds
on the components below, which keep their own licenses. Packages redistribute some of them,
so they install every license text named here into a `licenses/` folder next to the
executables (`fuckMail.app/Contents/Resources/licenses` on macOS).

## Bundled in the source tree

| Component | Used for | License | Text in this repository | Text in packages |
| --- | --- | --- | --- | --- |
| [litehtml](https://github.com/litehtml/litehtml) (vendored, locally modified) | Rendering received HTML mail | BSD-3-Clause | [gui/third_party/litehtml/LICENSE](gui/third_party/litehtml/LICENSE) | `LICENSE-litehtml.txt` |
| Gumbo HTML parser (part of litehtml) | HTML parsing inside litehtml | Apache-2.0 | [gui/third_party/litehtml/src/gumbo/LICENSE](gui/third_party/litehtml/src/gumbo/LICENSE) | `LICENSE-gumbo.txt` |
| [Geist and Geist Mono](https://github.com/vercel/geist-font) fonts | UI and fixed-pitch typefaces | SIL Open Font License 1.1 | [gui/fonts/LICENSE-Geist.txt](gui/fonts/LICENSE-Geist.txt) | `LICENSE-Geist.txt` |
| [rrule](https://github.com/fmeringdal/rust-rrule) 0.14.0 (vendored, locally modified) | Calendar recurrence expansion | MIT (upstream is MIT OR Apache-2.0) | [crates/fm-calendar/vendor/rrule/LICENSE-MIT](crates/fm-calendar/vendor/rrule/LICENSE-MIT) | `THIRD-PARTY-RUST.txt` |

Local modifications are described in [gui/third_party/litehtml/VENDORED.md](gui/third_party/litehtml/VENDORED.md)
and [crates/fm-calendar/vendor/rrule/FUCKMAIL-PATCH.md](crates/fm-calendar/vendor/rrule/FUCKMAIL-PATCH.md).

## Rust crates

`fm-engine` and `fm` statically link about 230 crates from crates.io, almost all
under MIT, Apache-2.0 or similar permissive licenses. The exact list for a build
comes from `Cargo.lock`. [scripts/third_party_notices.py](scripts/third_party_notices.py)
writes each crate's name, version, license and verbatim license files to
`THIRD-PARTY-RUST.txt`, which every package includes. To produce it locally:

```bash
python3 scripts/third_party_notices.py --output THIRD-PARTY-RUST.txt
```

## Qt

The GUI uses [Qt](https://www.qt.io/) 6 (Core, Gui, Qml, Quick, Quick Controls 2,
Network, Widgets, Svg and Multimedia) under the **GNU Lesser General Public License
v3**. That license covers Qt only, not fuckMail's code. Its text and the GPL-3.0 text it
builds on are in [gui/third_party/qt/](gui/third_party/qt/), and packages install them as
`LICENSE-Qt-LGPL-3.0.txt` and `LICENSE-Qt-GPL-3.0.txt`.

- Qt is linked dynamically, and packages ship the unmodified Qt libraries as separate
  files. You can replace them with your own build of the same Qt version.
- The Qt version is pinned in [scripts/toolchain.json](scripts/toolchain.json). Its source code
  is available from <https://download.qt.io/official_releases/qt/6.11/6.11.2/submodules/>
  (older versions move to <https://download.qt.io/archive/qt/>) and <https://code.qt.io/>.
- Qt includes third-party code of its own (for example FreeType, HarfBuzz, libpng,
  zlib and PCRE2). Qt documents these components and their licenses at
  <https://doc.qt.io/qt-6/licenses-used-in-qt.html>.
- Linux packages also ship the ICU libraries that Qt depends on, under the
  [Unicode License](https://github.com/unicode-org/icu/blob/main/LICENSE).

### FFmpeg

Qt Multimedia's FFmpeg backend brings the FFmpeg libraries (`avcodec`, `avformat`,
`avutil`, `swresample`, `swscale`). They are built by The Qt Company as shared
libraries under the **GNU Lesser General Public License v2.1 or later**
(`LICENSE-FFmpeg-LGPL-2.1.txt` in packages). FFmpeg source code is available from
<https://ffmpeg.org/download.html>.

## Platform runtimes (Windows)

Windows packages include Microsoft redistributable files. Each is distributed under
Microsoft's own terms, and none is modified:

- the Visual C++ runtime (`vcruntime140*.dll`, `msvcp140*.dll`, `concrt140.dll`);
- the Direct3D shader compilers that Qt deploys: `d3dcompiler_47.dll`, plus
  `dxcompiler.dll` and `dxil.dll` from the
  [DirectX Shader Compiler](https://github.com/microsoft/DirectXShaderCompiler).
