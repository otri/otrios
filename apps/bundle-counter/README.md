<!-- SPDX-License-Identifier: Apache-2.0 -->
# Bundle Counter

A standalone Canvas app with shared typed state and a pure Wasm increment/reset
reducer. It is intentionally absent from canvas-pack's built-in source registry.
It uses the existing Wasmi backend on browser and native hosts; this is not an AOT
compilation demo. The count is a bounded atomic register, not an operation-based
distributed counter promising concurrent increments are all retained.

Build and sign using the existing publisher key on the build machine:

```sh
cargo run -p otrios-dashboard --features canvas --example bundle_counter -- /tmp/counter-source
cargo run -p otrios-dashboard --features canvas --bin canvas-bundle -- \
  export /tmp/counter-source bundle-counter /tmp/counter-unsigned.otapp --revision 1
cargo run -p otrios-dashboard --features canvas --bin canvas-bundle -- \
  sign /tmp/counter-unsigned.otapp /tmp/bundle-counter.otapp \
  --key /PRIVATE/publisher/private.pem --openssl /PATH/TO/openssl
```

The example builder accepts an optional second argument, an increment step from
1 to 10, for upgrade qualification. Build a new output directory with step 2,
export with revision 2, sign and install it to test replacing UI/guest bytes while
preserving the state reference/schema. Normal use has step 1.

After deploying the bundle-capable host and qualifying its rollback point:

```sh
python3 scripts/install-app.py --runtime /PRIVATE/OTriOSDashboard \
  --checker /PRIVATE/build/mac-target/debug/canvas-bundle \
  --openssl /PATH/TO/openssl --bundle /tmp/bundle-counter.otapp
```

That stages a complete signed registry release with unchanged host binaries.
Add the established `--deploy-config /PRIVATE/config.json` to install on Frame.
Home discovers the new card; `/bundle-counter` opens it in the browser. The
private signing key is never copied to Frame.

Uninstallation uses `--remove bundle-counter` instead of `--bundle`; it removes
registration while retaining confirmed authority data. Install failures preserve
the current release. Follow the [Frame release and rollback workflow](../../docs/FRAME_HOSTING.md).
