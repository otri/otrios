<!-- SPDX-License-Identifier: Apache-2.0 -->
# Immutable public asset cache rule

Apply [the prepared rule](cloudflare-cache-rule.json) to the `otri.ca` zone's
`http_request_cache_settings` phase, or create the equivalent Cache Rule in the
Cloudflare dashboard. It makes public release web files and the public catalog
eligible for caching while respecting origin Cache-Control. Do not choose an
Edge TTL override that ignores origin no-store/error responses.

In the dashboard, choose the custom filter expression from the JSON, set cache
eligibility to **Eligible for cache**, and retain origin-controlled TTLs. Preserve
existing unrelated rules; review rule ordering for later overrides. No API token
or Cloudflare account connection is configured in the current cloud environment.

Only `/releases/.../web/...` and `/releases/.../apps/catalog.flex` match. Root and
deep-link HTML entries, `/release`, sessions, state, private app packages and
authenticated endpoints do not match. The authority's exact public file allowlist
still applies; unknown release files return no-store errors.

After applying, fetch the same immutable Wasm URL twice and confirm
`CF-Cache-Status: HIT`, then repeat timings. Check `/` and `/release` remain
no-store. A successful deployment or `Cache-Control: public` response alone does
not prove that the Cloudflare rule is active.
