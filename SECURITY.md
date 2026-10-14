# Security

on-air is LAN-only by design: the core listens on port 47990 on the local
network, remote clients pair with a one-time PIN and then use a bearer token,
and nothing talks to a server outside your network. The Sonos stream URL
carries a per-activation nonce and stops working on the next output switch.

If you find a vulnerability, please report it privately through
[GitHub's private vulnerability reporting](https://github.com/tsonubin/on-air/security/advisories/new)
rather than in a public issue. You will get an acknowledgement within a week.
Only the latest release is supported.
