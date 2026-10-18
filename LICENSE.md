# OTriOS license boundary

Original files in `crates/otrios-core/`, including runtime tests and future runtime FFI, use **LGPL-3.0-only**, except package manifests, which are Apache-2.0 build metadata. All other original files in this repository use **Apache-2.0**, unless a file explicitly states otherwise. Existing third-party notices and license texts retain their own terms.

This selects version 3 only for the initial scaffold. Any future switch to an or-later grant needs an explicit licensing decision. Each source file carries an SPDX identifier; Cargo package metadata identifies the code license.

Read [LGPLv3](LICENSES/LGPL-3.0-only.txt), its incorporated [GPLv3 terms](LICENSES/GPL-3.0-only.txt), and [Apache 2.0](LICENSES/Apache-2.0.txt). The license texts control.

## Runtime and implementation freedom

The intended boundary is a covered runtime library communicating through permissively licensed public contracts. Independent accelerator hardware, firmware, drivers, and implementations can remain proprietary where the applicable licenses permit. The Apache interface license does not relicense copied runtime code or automatically settle whether an integrated component is a derivative or combined work.

Distribution of modified covered runtime code and combined works must satisfy LGPLv3, including relevant source, notices, replacement/relinking, and installation-information obligations where applicable. Static Rust linking does not remove those obligations; decide a compliant distribution method before releasing combined binaries.

The LGPL does not require every internal change to be published or force contributors to submit changes upstream. The project encourages upstream contributions through public review. Linux and third-party dependencies keep their own licenses.

Reference texts: [GNU LGPLv3](https://www.gnu.org/licenses/lgpl-3.0.html) and [Apache License 2.0](https://www.apache.org/licenses/LICENSE-2.0).
