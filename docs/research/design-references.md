# Mockup references and technical implications

Received 2026-09-11 from the founder for stack evaluation. These are mood and layout references, explicitly not final or exact designs. Local copies preserve the supplied PNGs unchanged; text and example mailbox data inside them are mock content, not new requirements or implementation evidence.

| Reference | Observed treatment | Technical implication to test |
| --- | --- | --- |
| [Mockup 1](assets/2026-09-11/mockup-01.png) | Near-black canvas, mono metadata, large sans heading, account accents, two main panes and a bottom timeline with calendar lanes | Custom typography, bounded list/reader, flexible split layout and event geometry |
| [Mockup 2](assets/2026-09-11/mockup-02.png) | Serif-led layout, small caps, red wavy emphasis on delayed mail, review/delayed groups and a thin bottom time strip | Font shaping, text selection, custom state marks and compact event/now indicators |
| [Mockup 3](assets/2026-09-11/mockup-03.png) | Serif content with simpler navigation, visible unread/star counts, label chips, sidebar/list/reader and a sidebar agenda | Resizable regions, large-list virtualization, optional summary/source and attachment states |

The third image's sidebar agenda does not supersede the confirmed bottom strip on all mail screens. Nor do the images settle “Done/Finished” action semantics, split orientation/width, grouping rules, mail-arrival ticks, account/calendar overlays, fonts, final colors or summary availability. Preserve the accepted unread/star behavior and optional local assistance.

Across the references, the design challenge is custom text and layout rather than OS-style controls or complex 3D effects. Qt Quick/QML and Slint are both plausible visually. Prefer the stack that also solves rich email, command-line access, all required OSes and packaging with manageable integration work. Static mockups do not test HTML fidelity, million-message paging, accessibility, contrast, text editing or performance.

These references support the [current stack recommendation](desktop-stack.md). The [UI/UX brief](../ui-ux.md) remains the source of confirmed interaction requirements and open design choices. Bundled fonts, if chosen, need suitable redistribution rights and fallback/Unicode testing; font names cannot be established from these images alone.
