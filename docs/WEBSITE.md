# The Kibitz website

The site lives in `website/` at the repo root: hand-rolled static HTML/CSS, no
generator, no framework, no CDN. Everything it needs — fonts, screenshots,
styles — is checked in beside it, so the pages work offline and from any base
path. All internal links are **relative** (no leading `/`), because the site
initially serves from a sub-path: `https://avienu.github.io/kibitz/`.

## Layout

- `website/index.html` — landing page (identity, screenshots, download, club).
- `website/guide.html` — the rendered user guide. **Generated — do not edit by
  hand.** Edit `docs/USER_GUIDE.md` and rebuild (below).
- `website/style.css` — all styling; design tokens ported from
  `design/handoff-1/README.md` (dark default, light via
  `prefers-color-scheme`).
- `website/fonts/` — the same bundled woff2 files the app ships
  (`app/public/fonts/`), with their OFL license texts.
- `website/assets/` — real, unedited app screenshots copied from
  `docs/screenshots/` (run6 game view; run7 home and profile).

## Rebuilding the guide page

```
python3 scripts/site_build_guide.py           # rewrites website/guide.html
python3 scripts/site_build_guide.py --check   # exit 1 if the checked-in copy is stale
```

The script is stdlib-only and deterministic: it converts `docs/USER_GUIDE.md`
using the same markdown subset as the app's in-help renderer
(`app/src/lib/markdown.ts` — headings, lists, fences, inline code, bold) and
wraps it in the site chrome. Whenever `docs/USER_GUIDE.md` changes, rerun the
script and commit the regenerated `website/guide.html` in the same commit — CI
fails otherwise.

## How deployment works

`.github/workflows/pages.yml` deploys on every push to `main` that touches
`website/**`, the guide source, the build script, or the workflow itself
(plus manual runs via *Actions → pages → Run workflow*). It:

1. rebuilds `guide.html` and **diffs it against the checked-in copy** — a
   stale committed page fails the build;
2. uploads `website/` as the Pages artifact;
3. deploys with `actions/deploy-pages` to the `github-pages` environment.

One-time repo setup (already-done items are harmless to re-check): in the
GitHub repo, **Settings → Pages → Build and deployment → Source: GitHub
Actions**. The first successful run creates the `github-pages` environment
automatically.

## Placeholders the maintainer fills in

- **Donation URL** — `website/index.html`, the "Support the Temecula Chess
  Club" section: the link is `href="#DONATION-URL-PLACEHOLDER"` and styled
  with the `todo-link` class, which renders a visible red dashed border and a
  "TODO: set donation URL" tag. Replace the href with the real donation URL
  and change the class to `btn primary` (the TODO styling disappears with the
  class).
- **Build-from-source anchor** — `website/index.html` links to
  `https://github.com/avienu/kibitz#building-from-source`. When the public
  README lands, make sure it has a "Building from source" section (or update
  the anchor).

## Adding a custom domain later (Route 53)

No domain is secured yet. When one is chosen (`example.org` below — substitute
the real name):

1. **Tell Pages about the domain.** Repo **Settings → Pages → Custom domain**,
   enter `www.example.org` (or the apex), save. GitHub writes a `CNAME` file
   into the deployed site; with a workflow-deployed site like ours, also add
   the same single-line file at `website/CNAME` (contents: `www.example.org`)
   so it survives every deploy.
2. **Route 53 records** (hosted zone for `example.org`):
   - **Apex (`example.org`)** — an `A` record pointing at the GitHub Pages
     IPs: `185.199.108.153`, `185.199.109.153`, `185.199.110.153`,
     `185.199.111.153`; and an `AAAA` record with `2606:50c0:8000::153`,
     `2606:50c0:8001::153`, `2606:50c0:8002::153`, `2606:50c0:8003::153`.
     (Route 53 "ALIAS" targets only AWS resources, so for GitHub Pages the
     apex uses plain A/AAAA records. Verify the current IP list against
     GitHub's Pages custom-domain docs before creating the records.)
   - **`www.example.org`** — a `CNAME` record to `avienu.github.io`
     (the user domain, *not* `avienu.github.io/kibitz`).
3. **Verify + HTTPS.** Back in **Settings → Pages**, wait for the DNS check to
   pass, then tick **Enforce HTTPS** once GitHub has provisioned the
   certificate (can take up to an hour after DNS propagates).
4. Optionally add the domain under **Settings (org/user) → Pages → Verified
   domains** to stop takeovers if the Pages site is ever removed.
5. After the move, the site serves from the domain root instead of
   `/kibitz/` — the relative links keep working unchanged.

## Ground rules for site content

- No fake testimonials, no invented stats, no store-availability claims.
- Screenshots must be real, unedited captures of the app.
- The identity copy must describe what the product does **today**.
