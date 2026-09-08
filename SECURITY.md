# Security Policy

## Supported versions

`backpage` is pre-1.0. Only the latest release on `main` receives fixes.

| Version | Supported |
| ------- | --------- |
| 0.1.x   | yes       |

## Reporting a vulnerability

Report privately through GitHub Security Advisories:

<https://github.com/oddurs/backpage/security/advisories/new>

Please do not open a public issue for a security problem.

Include what you can — affected version, reproduction steps, and impact. A
proof of concept helps but is not required.

## What to expect

- Acknowledgement within 7 days.
- An assessment, and a fix or an explanation of why it is not a vulnerability,
  within 30 days.
- Credit in the release notes, unless you would rather stay anonymous.

## Threat model

`backpage` renders a read-only dashboard. It accepts no network input and no
user interaction. The parts most worth scrutiny are the ones that will read
system state and write to the desktop picture — neither is implemented yet.
