# Publishing the signed portable release

From the clean, reviewed and pushed release integration branch, one command owns
packaging, signing, draft/upload restart, publication, public verification and
fast-forward main promotion:

```sh
python3 -B scripts/distribution/release.py publish --private-key "$AM_RELEASE_SIGNING_KEY" --integration-branch integrate/v0.0.3 --notes-file /private/path/release-notes.md
```

Set `AM_RELEASE_SIGNING_KEY` to the dedicated external AM key. It must be an
ordinary current-user single-link owner-only file in a private directory, outside
the checkout and release output. The key is not copied into source or assets.
The local publisher requires Git, GitHub CLI with repository write access, Python,
curl and OpenSSH. Consumer installation does not require GitHub CLI or credentials.

The command creates one product release with four assets. It reuses an exact draft
or published version after byte/source readback and never clobbers assets or tags.
It publishes and verifies public assets before main exposes the signed-only
installer/updater. A concurrent incompatible main change is preserved and reported
as activation pending; it is never forced away. Repository main protection/rules
must match the reviewed promotion path before publication begins.

`build` and `render-bootstrap` are contributor/qualification helpers. They are not
additional steps in normal publication. Qualification uses isolated fixture keys
and local macOS/Linux journeys, not qualification releases or hosted CI matrices.
No installed `am` runtime is needed to publish from the reviewed source checkout.
