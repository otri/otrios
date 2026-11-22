<!-- SPDX-License-Identifier: Apache-2.0 -->
# Restore Frame Monitoring access — 2026-11-22

Frame authenticated correctly after the server cutover, but its existing
`frame-household` identity had no Operations membership on the new authority.
Only Public appeared on Home. The user authorized preauthenticated Monitoring
access; the existing offline provisioner added that identity as an Operations
Member. The existing human Admin remained unchanged. Operations stays closed,
and Frame received no Space administration privileges.

This was a membership-only configuration change. The authority and collector
were briefly quiesced for the existing one-shot `--provision-spaces` operation,
then their prior running configuration was restored. The plan selected only
Operations, retained its existing Admin and supplied no app imports/attachments.
Auth configuration, collector binding and active signed release were verified
unchanged. No token, login, trust root or host binary was created or replaced.
The private plan and provisioning log remain on the OrbStack authority under
`private/provision-frame-monitoring-member.*`.

## Live acceptance

- An authenticated `/spaces` request from Frame resolves `frame-household` with
  `member` access to Operations and its attached Monitoring app.
- An anonymous `/spaces` request still cannot discover Operations.
- Restarted `otrios-dashboard-direct` on Frame: its existing credential restored
  identity and the Home launcher displayed Operations/Monitoring automatically.
- Activated the Monitoring tile through the existing calibrated evdev reader.
  Actual native display readiness reported `app:monitoring/ui`, confirmed state,
  a ready live stream and zero pending commands, without interactive sign-in.
  Frame was left displaying Monitoring. This was synthetic device-input
  acceptance; no human touch test was performed.
- Active release remains
  `f129513ff9ad8280c1f458cf3c657a06c58dc6b0464dd3a553c7e8fc98b4323f`.

An older standalone handoff probe reported an incompatible state protocol; it
was not rebuilt or treated as acceptance. The currently deployed signed native
host supplied the successful end-to-end acceptance above. No build or broad test
suite was needed for this configuration-only grant. The aggregate acceptance
record is in the existing Mac checks directory as
`frame-monitoring-access.json`; credentials and private presentation bytes are
excluded.
