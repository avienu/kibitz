# Security Policy

## Supported versions

Kibitz is pre-1.0; only the latest release receives security fixes.

## Reporting a vulnerability

Please **do not open a public issue** for security problems.

Report vulnerabilities privately through GitHub's security advisories:
[github.com/avienu/kibitz/security/advisories/new](https://github.com/avienu/kibitz/security/advisories/new).
If you can't use GitHub, email
[contact@kibitzchess.org](mailto:contact@kibitzchess.org) instead.

Include what you can: affected version or commit, reproduction steps, and
impact as you understand it. You'll get an acknowledgment within a week; fixes
are coordinated through the advisory and credited to you unless you prefer
otherwise.

Things especially worth reporting for an app like this: anything that lets a
crafted PGN/SCID file or a synced network response execute code, read files
outside the app's data directory, or exfiltrate data; and anything that causes
the app to make network requests the user didn't ask for.
