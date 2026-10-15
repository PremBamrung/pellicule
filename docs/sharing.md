# Access & sharing

Pellicule runs **two HTTP listeners** from the same process. The split is the
security model: the public side physically cannot serve anything that wasn't
explicitly shared.

| | Private listener | Public listener |
|---|---|---|
| Default bind | `0.0.0.0:8080` | `127.0.0.1:8081` |
| Reached via | LAN, Tailscale (tailnet) | `tailscale funnel 8081` → `https://<host>.<tailnet>.ts.net` |
| Serves | Whole library, admin (share management), originals | Only albums behind a valid share token |
| Auth | Network-level (tailnet) — optional login later | Token in URL, optional password, optional expiry |
| GPS / location | Shown if present | **Never** exposed (stripped from metadata payloads) |

## Share links

```
https://nas.tailxxxx.ts.net/s/<token>/                    → album page
https://nas.tailxxxx.ts.net/s/<token>/day-2               → nested album (if allowed)
https://nas.tailxxxx.ts.net/s/<token>/d/<key>-l.v1.webp   → derivative
```

- `token`: 128-bit from the OS RNG, base64url (22 chars). **Only its SHA-256 is
  stored**, so a leaked database doesn't leak working links.
- Scoped to one album path (`include_subalbums`, default true) or an explicit
  list of media.
- Optional **password** (Argon2id). Checked **once** at `/s/<token>/unlock`,
  which sets an HMAC-signed, `HttpOnly`, `Secure`, `SameSite=Lax` cookie scoped
  to `/s/<token>`. The cookie embeds a fingerprint of the password hash, so
  changing the password revokes every grant. Media requests only verify the HMAC.
- Optional **expiry**; expired or revoked tokens return 404 (not 403 — no oracle).
- Flags: `allow_download` (originals; default **off**), `show_exif` (default on),
  `show_recipe` (default on), `max_variant` (default `l`, 2048 px).
- Created and revoked from the private UI (and/or `pellicule share` CLI).

## Funnel constraints

Tailscale Funnel goes through relays with limited, non-configurable bandwidth
(community reports of ~7–10 Mbps — to be measured on our setup). Consequences:

- Share pages only ever reference web derivatives, capped at `l` (2048 px WebP,
  ~250–500 KB), and 720p video previews (~2.5 Mbps max) with range requests.
- Aggressive caching: derivative URLs are content-addressed and served with
  `Cache-Control: public, max-age=31536000, immutable`; HTML gets ETags, so
  repeat visits are 304s.
- HTML for share pages is small and server-rendered; no large JS bundle.
- Funnel only exposes ports 443, 8443 and 10000 publicly; it maps them to our
  local public listener.

## Hardening checklist (public listener)

- Only `/s/…` routes and static assets are mounted; everything else 404s.
- **Never treat loopback as trusted**: Funnel traffic arrives from localhost
  (HomeGallery's `allow 127/8` rule turned that into an auth bypass).
- Every media request checks that the item belongs to the share's scope —
  unguessable content-key URLs are not authorization.
- Token lookup by hash; rate limiting on the public listener, stricter on `/unlock`.
- No directory listing, no path parameters that reach the filesystem directly —
  all media are resolved through the database by ID, and the ID must belong to
  the share's album subtree.
- Security headers: CSP (self only), `Referrer-Policy: no-referrer` (keeps tokens
  out of referrers), `X-Content-Type-Options: nosniff`.
