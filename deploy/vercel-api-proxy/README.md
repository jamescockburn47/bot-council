# vercel-api-proxy

Source for the Vercel project `lqcouncil-api-proxy`, which fronts
`https://api.lqcouncil.com` and rewrites every path onto the Tailscale
Funnel URL exposing EVO's `:3100`.

The Vercel project has no git integration — deploys are CLI-driven. Keep
this directory as the authoritative source.

## Deploy

```bash
cd deploy/vercel-api-proxy
vercel link --yes --project lqcouncil-api-proxy
vercel deploy --yes                     # preview
# Smoke-test the preview URL:
curl https://<preview>.vercel.app/health     # expect 200
curl -o /dev/null -w '%{http_code}\n' https://<preview>.vercel.app/me   # expect 401
# Promote + re-alias (both required):
vercel deploy --prod --yes
# Find the new prod deployment URL from the last output, then:
vercel alias set <new-prod-url> api.lqcouncil.com
# Verify:
curl https://api.lqcouncil.com/health
vercel alias ls | grep api.lqcouncil.com
```

**Important:** `vercel deploy --prod` promotes to production but does NOT
re-alias `api.lqcouncil.com` automatically. Always follow with
`vercel alias set`, then verify.
