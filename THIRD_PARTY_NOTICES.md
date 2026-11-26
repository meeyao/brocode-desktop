# Third-Party Notices

Brocode incorporates the third-party software, fonts, dictionaries, and libraries listed below. Original copyright notices and license texts are reproduced verbatim for attribution and redistribution compliance.

---

## 1. UI & Terminal Fonts (SIL OFL 1.1 / MIT)

Standalone license files also ship alongside the font assets in `apps/desktop/assets/fonts/` (and `fonts/` in browser builds). The full SIL Open Font License, Version 1.1 text appears in [Section 2](#sil-open-font-license-version-11).

- **Inter** (`https://github.com/rsms/inter`) — SIL Open Font License 1.1
  - `apps/desktop/assets/fonts/Inter-LICENSE.txt`
  - `Copyright (c) 2016 The Inter Project Authors (https://github.com/rsms/inter)`
- **IBM Plex Mono** (`https://github.com/IBM/plex`) — SIL Open Font License 1.1
  - `apps/desktop/assets/fonts/IBMPlexMono-LICENSE.txt`
  - `Copyright © 2017 IBM Corp. with Reserved Font Name "Plex"`
- **CommitMono** (`https://github.com/eigilnikolajsen/commit-mono`) — SIL Open Font License 1.1
  - `apps/desktop/assets/fonts/CommitMono-LICENSE.txt`
  - `Copyright (c) 2023 Eigil Nikolajsen (eigi0088@gmail.com)`
- **JetBrains Mono Nerd Font Mono** (`https://github.com/JetBrains/JetBrainsMono`, patched with Nerd Fonts v3.4.0, `https://github.com/ryanoasis/nerd-fonts`) — SIL Open Font License 1.1 & MIT License
  - `apps/desktop/assets/fonts/JetBrainsMonoNerdFontMono-LICENSE.txt`
  - `Copyright 2020 The JetBrains Mono Project Authors (https://github.com/JetBrains/JetBrainsMono)`
  - `JetBrains Mono is a trademark of JetBrains s.r.o.`
  - `Copyright (c) 2014, Ryan L McIntyre (https://ryanlmcintyre.com).` (SIL OFL 1.1)
  - `Copyright (c) 2014 Ryan L McIntyre` (Nerd Fonts, MIT License — full text in `apps/desktop/assets/fonts/JetBrainsMonoNerdFontMono-LICENSE.txt`)

---

## 2. Office Preview Fonts (SIL OFL 1.1)

Word, Excel, and PowerPoint previews (`crates/office-preview`) bundle the following font families via `betteroffice-fonts` (`fonts-latin` and `fonts-intl`) and `betteroffice-base-latin`. Every family below is licensed under the SIL Open Font License, Version 1.1 (reproduced in full once below).

### Latin, Metric-Compatible, Presentation, Arabic & Hebrew Families (`betteroffice-fonts-latin`)

1. **Carlito** (`https://github.com/googlefonts/carlito`)
   - `Copyright 2013 The Carlito Project Authors (https://github.com/googlefonts/carlito), with Reserved Font Name "Carlito"`
2. **Selawik** (`https://github.com/microsoft/Selawik`)
   - `Copyright 2015, Microsoft Corporation (www.microsoft.com), with Reserved Font Name Selawik. All Rights Reserved. Selawik is a trademark of Microsoft Corporation in the United States and/or other countries.`
3. **Caladea** (`https://github.com/huertatipografica/Caladea`)
   - `Copyright 2012 The Caladea Project Authors (https://github.com/huertatipografica/Caladea)`
4. **Liberation Sans, Liberation Serif, Liberation Mono** (`https://github.com/liberationfonts/liberation-fonts`)
   - `Digitized data copyright (c) 2010 Google Corporation`
   - `with Reserved Font Arimo, Tinos and Cousine.`
   - `Copyright (c) 2012 Red Hat, Inc.`
   - `with Reserved Font Name Liberation.`
5. **Gelasio** (`https://github.com/SorkinType/Gelasio`)
   - `Copyright 2022 The Gelasio Project Authors (https://github.com/SorkinType/Gelasio)`
6. **Comic Relief** (`https://github.com/loudifier/Comic-Relief`)
   - `Copyright 2013 The Comic Relief Project Authors (https://github.com/loudifier/Comic-Relief)`
7. **Inter** (`https://github.com/rsms/inter`)
   - `Copyright 2020 The Inter Project Authors (https://github.com/rsms/inter)`
8. **Roboto** (`https://github.com/googlefonts/roboto-classic`)
   - `Copyright 2011 The Roboto Project Authors (https://github.com/googlefonts/roboto-classic)`
9. **Source Sans 3** (`https://github.com/adobe-fonts/source-sans`)
   - `Copyright 2010-2020 Adobe (http://www.adobe.com/), with Reserved Font Name 'Source'. All Rights Reserved. Source is a trademark of Adobe in the United States and/or other countries.`
10. **DM Sans** (`https://github.com/googlefonts/dm-fonts`)
    - `Copyright 2014 The DM Sans Project Authors (https://github.com/googlefonts/dm-fonts)`
11. **DM Serif Display** (`https://github.com/googlefonts/dm-fonts`)
    - `Copyright 2014-2018 Adobe (http://www.adobe.com/), with Reserved Font Name 'Source'. All Rights Reserved. Source is a trademark of Adobe in the United States and/or other countries. Copyright 2019 Google LLC.`
12. **Open Sans** (`https://github.com/googlefonts/opensans`)
    - `Copyright 2020 The Open Sans Project Authors (https://github.com/googlefonts/opensans)`
13. **Montserrat** (`https://github.com/JulietaUla/Montserrat`)
    - `Copyright 2024 The Montserrat.Git Project Authors (https://github.com/JulietaUla/Montserrat.git)`
14. **Poppins** (`https://github.com/itfoundry/Poppins`)
    - `Copyright 2020 The Poppins Project Authors (https://github.com/itfoundry/Poppins)`
15. **Oswald** (`https://github.com/googlefonts/OswaldFont`)
    - `Copyright 2016 The Oswald Project Authors (https://github.com/googlefonts/OswaldFont)`
16. **Heebo** (`https://github.com/OdedEzer/heebo`)
    - `Copyright 2014 The Heebo Project Authors (https://github.com/OdedEzer/heebo)`
17. **Noto Sans Arabic, Noto Naskh Arabic** (`https://github.com/notofonts/arabic`)
    - `Copyright 2022 The Noto Project Authors (https://github.com/notofonts/arabic)`
18. **Noto Sans Hebrew** (`https://github.com/notofonts/hebrew`)
    - `Copyright 2022 The Noto Project Authors (https://github.com/notofonts/hebrew)`

### Base Latin & International Script Coverage (`betteroffice-base-latin`, `betteroffice-fonts-intl`)

- **Noto Sans** (`https://github.com/notofonts/latin-greek-cyrillic`)
  - `Copyright 2022 The Noto Project Authors (https://github.com/notofonts/latin-greek-cyrillic)`
- **Noto Sans Symbols 2** (`https://github.com/notofonts/symbols`)
  - `Copyright 2022 The Noto Project Authors (https://github.com/notofonts/symbols)`
- **Noto International Script Faces** (`https://github.com/notofonts` — Noto Sans Armenian, Bengali, Cherokee, Devanagari, Ethiopic, Georgian, Gujarati, Gurmukhi, Kannada, Khmer, Lao, Lao Looped, Malayalam, Mongolian, Myanmar, Oriya, Sinhala, Syriac, Tamil, Telugu, Thaana, Thai, Tifinagh; Noto Serif Tibetan)
  - `The Noto Project Authors (https://github.com/notofonts). Font-specific copyright notices are retained in the font name tables.`

### SIL Open Font License, Version 1.1

```text
This Font Software is licensed under the SIL Open Font License, Version 1.1.
This license is copied below, and is also available with a FAQ at:
https://scripts.sil.org/OFL


-----------------------------------------------------------
SIL OPEN FONT LICENSE Version 1.1 - 26 February 2007
-----------------------------------------------------------

PREAMBLE
The goals of the Open Font License (OFL) are to stimulate worldwide
development of collaborative font projects, to support the font creation
efforts of academic and linguistic communities, and to provide a free and
open framework in which fonts may be shared and improved in partnership
with others.

The OFL allows the licensed fonts to be used, studied, modified and
redistributed freely as long as they are not sold by themselves. The
fonts, including any derivative works, can be bundled, embedded, 
redistributed and/or sold with any software provided that any reserved
names are not used by derivative works. The fonts and derivatives,
however, cannot be released under any other type of license. The
requirement for fonts to remain under this license does not apply
to any document created using the fonts or their derivatives.

DEFINITIONS
"Font Software" refers to the set of files released by the Copyright
Holder(s) under this license and clearly marked as such. This may
include source files, build scripts and documentation.

"Reserved Font Name" refers to any names specified as such after the
copyright statement(s).

"Original Version" refers to the collection of Font Software components as
distributed by the Copyright Holder(s).

"Modified Version" refers to any derivative made by adding to, deleting,
or substituting -- in part or in whole -- any of the components of the
Original Version, by changing formats or by porting the Font Software to a
new environment.

"Author" refers to any designer, engineer, programmer, technical
writer or other person who contributed to the Font Software.

PERMISSION & CONDITIONS
Permission is hereby granted, free of charge, to any person obtaining
a copy of the Font Software, to use, study, copy, merge, embed, modify,
redistribute, and sell modified and unmodified copies of the Font
Software, subject to the following conditions:

1) Neither the Font Software nor any of its individual components,
in Original or Modified Versions, may be sold by itself.

2) Original or Modified Versions of the Font Software may be bundled,
redistributed and/or sold with any software, provided that each copy
contains the above copyright notice and this license. These can be
included either as stand-alone text files, human-readable headers or
in the appropriate machine-readable metadata fields within text or
binary files as long as those fields can be easily viewed by the user.

3) No Modified Version of the Font Software may use the Reserved Font
Name(s) unless explicit written permission is granted by the corresponding
Copyright Holder. This restriction only applies to the primary font name as
presented to the users.

4) The name(s) of the Copyright Holder(s) or the Author(s) of the Font
Software shall not be used to promote, endorse or advertise any
Modified Version, except to acknowledge the contribution(s) of the
Copyright Holder(s) and the Author(s) or with their explicit written
permission.

5) The Font Software, modified or unmodified, in part or in whole,
must be distributed entirely under this license, and must not be
distributed under any other license. The requirement for fonts to
remain under this license does not apply to any document created
using the Font Software.

TERMINATION
This license becomes null and void if any of the above conditions are
not met.

DISCLAIMER
THE FONT SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO ANY WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT
OF COPYRIGHT, PATENT, TRADEMARK, OR OTHER RIGHT. IN NO EVENT SHALL THE
COPYRIGHT HOLDER BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
INCLUDING ANY GENERAL, SPECIAL, INDIRECT, INCIDENTAL, OR CONSEQUENTIAL
DAMAGES, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
FROM, OUT OF THE USE OR INABILITY TO USE THE FONT SOFTWARE OR FROM
OTHER DEALINGS IN THE FONT SOFTWARE.
```

---

## 3. Office Preview Engines (Apache License 2.0)

- **BetterOffice** (`https://github.com/openooxml/betteroffice` — `betteroffice-*` crates in `crates/office-preview`) — Apache License 2.0 (with BSD-3-Clause Zstandard support in `betteroffice-fonts`)
  - Verbatim `NOTICE`:
    ```text
    BetterOffice
    Copyright 2026 Elia Hilse, The OpenOOXML Project
    Copyright 2026 The OpenOOXML contributors
    ```
- **eigenpal docx editor** (`https://github.com/eigenpal/docx-editor`) — Apache License 2.0
  - `Copyright 2026 EigenPal Inc.`

### Apache License, Version 2.0

```text
                                 Apache License
                           Version 2.0, January 2004
                        http://www.apache.org/licenses/

   TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

   1. Definitions.

      "License" shall mean the terms and conditions for use, reproduction,
      and distribution as defined by Sections 1 through 9 of this document.

      "Licensor" shall mean the copyright owner or entity authorized by
      the copyright owner that is granting the License.

      "Legal Entity" shall mean the union of the acting entity and all
      other entities that control, are controlled by, or are under common
      control with that entity. For the purposes of this definition,
      "control" means (i) the power, direct or indirect, to cause the
      direction or management of such entity, whether by contract or
      otherwise, or (ii) ownership of fifty percent (50%) or more of the
      outstanding shares, or (iii) beneficial ownership of such entity.

      "You" (or "Your") shall mean an individual or Legal Entity
      exercising permissions granted by this License.

      "Source" form shall mean the preferred form for making modifications,
      including but not limited to software source code, documentation
      source, and configuration files.

      "Object" form shall mean any form resulting from mechanical
      transformation or translation of a Source form, including but
      not limited to compiled object code, generated documentation,
      and conversions to other media types.

      "Work" shall mean the work of authorship, whether in Source or
      Object form, made available under the License, as indicated by a
      copyright notice that is included in or attached to the work
      (an example is provided in the Appendix below).

      "Derivative Works" shall mean any work, whether in Source or Object
      form, that is based on (or derived from) the Work and for which the
      editorial revisions, annotations, elaborations, or other modifications
      represent, as a whole, an original work of authorship. For the purposes
      of this License, Derivative Works shall not include works that remain
      separable from, or merely link (or bind by name) to the interfaces of,
      the Work and Derivative Works thereof.

      "Contribution" shall mean any work of authorship, including
      the original version of the Work and any modifications or additions
      to that Work or Derivative Works thereof, that is intentionally
      submitted to Licensor for inclusion in the Work by the copyright owner
      or by an individual or Legal Entity authorized to submit on behalf of
      the copyright owner. For the purposes of this definition, "submitted"
      means any form of electronic, verbal, or written communication sent
      to the Licensor or its representatives, including but not limited to
      communication on electronic mailing lists, source code control systems,
      and issue tracking systems that are managed by, or on behalf of, the
      Licensor for the purpose of discussing and improving the Work, but
      excluding communication that is conspicuously marked or otherwise
      designated in writing by the copyright owner as "Not a Contribution."

      "Contributor" shall mean Licensor and any individual or Legal Entity
      on behalf of whom a Contribution has been received by Licensor and
      subsequently incorporated within the Work.

   2. Grant of Copyright License. Subject to the terms and conditions of
      this License, each Contributor hereby grants to You a perpetual,
      worldwide, non-exclusive, no-charge, royalty-free, irrevocable
      copyright license to reproduce, prepare Derivative Works of,
      publicly display, publicly perform, sublicense, and distribute the
      Work and such Derivative Works in Source or Object form.

   3. Grant of Patent License. Subject to the terms and conditions of
      this License, each Contributor hereby grants to You a perpetual,
      worldwide, non-exclusive, no-charge, royalty-free, irrevocable
      (except as stated in this section) patent license to make, have made,
      use, offer to sell, sell, import, and otherwise transfer the Work,
      where such license applies only to those patent claims licensable
      by such Contributor that are necessarily infringed by their
      Contribution(s) alone or by combination of their Contribution(s)
      with the Work to which such Contribution(s) was submitted. If You
      institute patent litigation against any entity (including a
      cross-claim or counterclaim in a lawsuit) alleging that the Work
      or a Contribution incorporated within the Work constitutes direct
      or contributory patent infringement, then any patent licenses
      granted to You under this License for that Work shall terminate
      as of the date such litigation is filed.

   4. Redistribution. You may reproduce and distribute copies of the
      Work or Derivative Works thereof in any medium, with or without
      modifications, and in Source or Object form, provided that You
      meet the following conditions:

      (a) You must give any other recipients of the Work or
          Derivative Works a copy of this License; and

      (b) You must cause any modified files to carry prominent notices
          stating that You changed the files; and

      (c) You must retain, in the Source form of any Derivative Works
          that You distribute, all copyright, patent, trademark, and
          attribution notices from the Source form of the Work,
          excluding those notices that do not pertain to any part of
          the Derivative Works; and

      (d) If the Work includes a "NOTICE" text file as part of its
          distribution, then any Derivative Works that You distribute must
          include a readable copy of the attribution notices contained
          within such NOTICE file, excluding those notices that do not
          pertain to any part of the Derivative Works, in at least one
          of the following places: within a NOTICE text file distributed
          as part of the Derivative Works; within the Source form or
          documentation, if provided along with the Derivative Works; or,
          within a display generated by the Derivative Works, if and
          wherever such third-party notices normally appear. The contents
          of the NOTICE file are for informational purposes only and
          do not modify the License. You may add Your own attribution
          notices within Derivative Works that You distribute, alongside
          or as an addendum to the NOTICE text from the Work, provided
          that such additional attribution notices cannot be construed
          as modifying the License.

      You may add Your own copyright statement to Your modifications and
      may provide additional or different license terms and conditions
      for use, reproduction, or distribution of Your modifications, or
      for any such Derivative Works as a whole, provided Your use,
      reproduction, and distribution of the Work otherwise complies with
      the conditions stated in this License.

   5. Submission of Contributions. Unless You explicitly state otherwise,
      any Contribution intentionally submitted for inclusion in the Work
      by You to the Licensor shall be under the terms and conditions of
      this License, without any additional terms or conditions.
      Notwithstanding the above, nothing herein shall supersede or modify
      the terms of any separate license agreement you may have executed
      with Licensor regarding such Contributions.

   6. Trademarks. This License does not grant permission to use the trade
      names, trademarks, service marks, or product names of the Licensor,
      except as required for reasonable and customary use in describing the
      origin of the Work and reproducing the content of the NOTICE file.

   7. Disclaimer of Warranty. Unless required by applicable law or
      agreed to in writing, Licensor provides the Work (and each
      Contributor provides its Contributions) on an "AS IS" BASIS,
      WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
      implied, including, without limitation, any warranties or conditions
      of TITLE, NON-INFRINGEMENT, MERCHANTABILITY, or FITNESS FOR A
      PARTICULAR PURPOSE. You are solely responsible for determining the
      appropriateness of using or redistributing the Work and assume any
      risks associated with Your exercise of permissions under this License.

   8. Limitation of Liability. In no event and under no legal theory,
      whether in tort (including negligence), contract, or otherwise,
      unless required by applicable law (such as deliberate and grossly
      negligent acts) or agreed to in writing, shall any Contributor be
      liable to You for damages, including any direct, indirect, special,
      incidental, or consequential damages of any character arising as a
      result of this License or out of the use or inability to use the
      Work (including but not limited to damages for loss of goodwill,
      work stoppage, computer failure or malfunction, or any and all
      other commercial damages or losses), even if such Contributor
      has been advised of the possibility of such damages.

   9. Accepting Warranty or Additional Liability. While redistributing
      the Work or Derivative Works thereof, You may choose to offer,
      and charge a fee for, acceptance of support, warranty, indemnity,
      or other liability obligations and/or rights consistent with this
      License. However, in accepting such obligations, You may act only
      on Your own behalf and on Your sole responsibility, not on behalf
      of any other Contributor, and only if You agree to indemnify,
      defend, and hold each Contributor harmless for any liability
      incurred by, or claims asserted against, such Contributor by reason
      of your accepting any such warranty or additional liability.

   END OF TERMS AND CONDITIONS
```

---

## 4. Spellcheck Dictionary

The browser client (`apps/web`) ships the **en_US Hunspell Dictionary** (Version `2020.12.07`, derived from **SCOWL**, `http://wordlist.sourceforge.net`, distributed via `wooorm/dictionaries` `dictionary-en`) in `apps/web/spell/en-US.aff` and `apps/web/spell/en-US.dic`.

The complete license, source credits, and verbatim component notices ship in `apps/web/spell/en-US.LICENSE.txt` (and `spell/en-US.LICENSE.txt` in browser distributions), including:

- **SCOWL / Collective Work:** `Copyright 2000-2018 by Kevin Atkinson` (MIT-style permissive license) and `Copyright 2016 by Benjamin Titze` (VarCon)
- **Ispell (affix & word list):** `Copyright 1993, Geoff Kuenning, Granada Hills, CA` (BSD license)
- **WordNet 1.6:** `WordNet 1.6 Copyright 1997 by Princeton University. All rights reserved.`
- **UK Advanced Cryptics Dictionary (UKACD):**
  ```text
  Copyright (c) J Ross Beresford 1993-1999. All Rights Reserved.

  The following restriction is placed on the use of this publication:
  if The UK Advanced Cryptics Dictionary is used in a software package
  or redistributed in any form, the copyright notice must be
  prominently displayed and the text of this document must be included
  verbatim.

  There are no other restrictions: I would like to see the list
  distributed as widely as possible.
  ```

---

## 5. Native & Vendored Engines

- **mimalloc** (`https://github.com/microsoft/mimalloc`, mimalloc 3.3, vendored via `vendor/libmimalloc-sys` 0.1.49) — MIT License
  - `Copyright (c) 2018-2025 Microsoft Corporation, Daan Leijen` (`vendor/libmimalloc-sys/c_src/mimalloc/v3/LICENSE`)
  - Rust bindings (`libmimalloc-sys`): `Copyright 2019 Octavian Oncescu` (MIT, `vendor/libmimalloc-sys/LICENSE.txt`)
  - **Local modification:** `vendor/libmimalloc-sys.patch` patches `c_src/mimalloc/v3/include/mimalloc/types.h` and `c_src/mimalloc/v3/src/arena.c` so small (64 KiB) pages commit on demand in 16 KiB steps.
- **Ghostty / `libghostty-vt`** (`https://github.com/ghostty-org/ghostty`, `v1.4.0`, commit `83fb95624b`, statically linked by `crates/terminal`) — MIT License
  - `Copyright (c) 2024 Mitchell Hashimoto, Ghostty contributors`
- **Chromium Embedded Framework (CEF)** (`https://github.com/chromiumembedded/cef`, used by the on-demand `apps/browser-engine` sidecar) — BSD-3-Clause License
  - `Copyright (c) 2008-2020 Marshall A. Greenblatt. Portions Copyright (c) 2006-2009 Google Inc. All rights reserved.` (The Chromium Embedded Framework Authors)
- **KaTeX Math Fonts** (`https://github.com/KaTeX/KaTeX`, 20 TrueType faces with Reserved Font Names `KaTeX_*`, embedded via `rune-kit` `math` feature from `ratex-katex-fonts 0.1.16`) — SIL Open Font License, Version 1.1 (full license text in Section 2 above)
  - `Copyright (c) 2009-2010 Design Science, Inc.; Copyright (c) 2014-2018 Khan Academy (KaTeX)`
- **Sandboxed LaTeX Display-Math Typesetter** (`ratex-math.wasm`, embedded via `rune-kit` `math` feature):
  - **RaTeX** (`ratex-parser`, `ratex-layout`, `ratex-types`, `ratex-font`, `ratex-lexer` `0.1.16`, `https://github.com/erweixin/RaTeX`) — MIT License
  - **unicode-normalization** (`0.1.26`, compiled inside `ratex-math.wasm`, `https://github.com/unicode-rs/unicode-normalization`) — (MIT OR Apache-2.0) AND Unicode-3.0
  - **wasmi** (`wasmi`, `wasmi_core`, `wasmi_ir`, `wasmi_collections` `1.1.0`, native sandbox interpreter, `https://github.com/wasmi-labs/wasmi`) — MIT OR Apache-2.0

```text
Copyright (c) 2008-2020 Marshall A. Greenblatt. Portions Copyright (c)
2006-2009 Google Inc. All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are
met:

   * Redistributions of source code must retain the above copyright
notice, this list of conditions and the following disclaimer.
   * Redistributions in binary form must reproduce the above
copyright notice, this list of conditions and the following disclaimer
in the documentation and/or other materials provided with the
distribution.
   * Neither the name of Google Inc. nor the name Chromium Embedded
Framework nor the names of its contributors may be used to endorse
or promote products derived from this software without specific prior
written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
"AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR
A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT
OWNER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT
LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
(INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

---

## 6. MPL-2.0 Rust Dependencies

The following 14 crates in `Cargo.lock` are licensed under the **Mozilla Public License, Version 2.0** (`https://mozilla.org/MPL/2.0/`). Brocode uses these crates unmodified; their complete source code is publicly available at the upstream repositories below and on crates.io (`https://crates.io/crates/<name>/<version>`). In accordance with MPL-2.0 Section 3.2(a), any covered source code remains available under the terms of the MPL-2.0.

| Crate | Version | Source |
| --- | --- | --- |
| `avif-parse` | `2.1.0` | `https://github.com/kornelski/avif-parse` |
| `cssparser` | `0.35.0` | `https://github.com/servo/rust-cssparser` |
| `cssparser-macros` | `0.6.1` | `https://github.com/servo/rust-cssparser` |
| `dtoa-short` | `0.3.5` | `https://github.com/upsuper/dtoa-short` |
| `opuscule` | `0.2.2` | `https://github.com/bryal/opuscule` |
| `selectors` | `0.31.0` | `https://github.com/servo/stylo` |
| `spellbook` | `0.4.2` | `https://github.com/helix-editor/spellbook` |
| `symphonia` | `0.5.5` | `https://github.com/pdeljanov/Symphonia` |
| `symphonia-codec-vorbis` | `0.5.5` | `https://github.com/pdeljanov/Symphonia` |
| `symphonia-core` | `0.5.5` | `https://github.com/pdeljanov/Symphonia` |
| `symphonia-format-mkv` | `0.5.5` | `https://github.com/pdeljanov/Symphonia` |
| `symphonia-format-ogg` | `0.5.5` | `https://github.com/pdeljanov/Symphonia` |
| `symphonia-metadata` | `0.5.5` | `https://github.com/pdeljanov/Symphonia` |
| `symphonia-utils-xiph` | `0.5.5` | `https://github.com/pdeljanov/Symphonia` |

---

## 7. Other Rust Dependencies

All remaining external Rust dependencies locked in `Cargo.lock` and `apps/browser-engine/Cargo.lock` are permissively licensed under MIT, Apache-2.0 (including `Apache-2.0 WITH LLVM-exception`), BSD-2-Clause, BSD-3-Clause, 0BSD, ISC, Zlib, Unicode-3.0, BSL-1.0, CC0-1.0, Unlicense, CDLA-Permissive-2.0, or bzip2-1.0.6. Multi-licensed crates that include a copyleft option alongside permissive options (such as `r-efi`: `MIT OR Apache-2.0 OR LGPL-2.1-or-later`) are used under their MIT or Apache-2.0 license.
