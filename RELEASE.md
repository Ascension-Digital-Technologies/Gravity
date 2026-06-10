# Release Process

1. Update `VERSION`, `CHANGELOG.md`, and relevant docs.
2. Run the release gate:

```bash
./scripts/release-gate.sh
```

3. Verify generated reports under `runtime/reports/`.
4. Package the repository without `target/`, runtime outputs, secrets, or local config.
5. Tag the release using the version from `VERSION`.

Release manifests are archived under `docs/releases/`.
