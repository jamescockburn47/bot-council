# RUNBOOK — what to do when something breaks

Written for the operator, not an engineer. Each incident is four moves:
what it looks like, the one thing to do, how you know it worked, and what
to hand an agent if it didn't. Commands run from the ship anchor checkout
(`reverent-goldwasser`) on the dev machine unless marked "on EVO" (SSH:
`ssh james@100.90.66.54`).

**Maintenance rule:** any PR that changes a command named here must update
this file. CI checks that every `./scripts/*.sh` named here still exists.

---

## 1. The site is down or throwing errors after a deploy

**Looks like:** lqcouncil.com erroring or blank right after a `ship.sh`,
where it was fine before.

**Do this:**
```
./scripts/rollback.sh
```

**It worked if:** the script's health poll ends green and
`https://lqcouncil.com/api/health` shows `{"status":"ok"}` in your browser.
Rollback takes about ten seconds and restores the previous binary.

**If not, hand an agent:** "The site was broken after a deploy and
rollback.sh did not restore it. The rollback output was: <paste>. The
journal (/admin/events) shows: <paste recent problem entries>."

## 2. Summaries are failing, empty, or nonsense (the AI service)

**Looks like:** debates finish but the analysis/summary is missing or a
stub; the journal shows `synthesis_fallback` or `debate_failed` entries
mentioning the summariser.

**Do this** `[manual]` (on EVO): the hosted model may be down — switch to
the local fallback model:
```
sudo nano /etc/bot-council.env     # blank the three APP__MODELS__*_BASE_URL lines
ps aux | grep llama-server          # confirm the local model is running
sudo systemctl restart bot-council
```

**It worked if:** the next debate's summary is substantive, and the journal
shows a `model_route_changed` entry recording the switch.

**If not, hand an agent:** "Summaries are failing. I switched the model
route to local per RUNBOOK §2 and it did not help. Journal entries:
<paste>. The route change entry says: <paste>."

## 3. The public site is unreachable but debates still run

**Looks like:** lqcouncil.com times out in the browser, but you can reach
the app on EVO directly (`curl http://localhost:3100/api/health` on EVO
returns ok). That is the tunnel, not the app.

**Do this** `[manual]` (on EVO):
```
sudo systemctl restart sovren-cloudflared
```

**It worked if:** `https://lqcouncil.com/api/health` loads within a minute.

**If not, hand an agent:** "The Cloudflare tunnel is down and restarting
sovren-cloudflared did not fix it. `systemctl status sovren-cloudflared`
says: <paste>."

## 4. Deploying a new version

**Looks like:** not an incident — you want merged changes live.

**Do this:**
```
./scripts/ship.sh
```

**It worked if:** all seven stages print green and the final public smoke
check passes. The journal will show a `service_started` entry for the new
version.

**If not, hand an agent:** "ship.sh failed at stage <n> with: <paste>. Do
not retry blindly — diagnose first." (A failed ship leaves the old version
running; nothing is broken until stage 5 replaces the binary, and
rollback.sh restores it if so.)

## 5. Summaries need rebuilding (after a prompt or model change)

**Looks like:** not an incident — old debates should pick up improved
summary logic.

**Do this** `[manual]` (on EVO):
```
bash /home/james/resynth-launch.sh
```

**It worked if:** the journal shows a `resynth_run` entry with the counts,
and old debates show refreshed summaries.

**If not, hand an agent:** "The resynth batch failed or produced worse
summaries. The resynth_run journal entry says: <paste>."

## 6. EVO rebooted (power cut, update) — is everything back?

**Looks like:** the box restarted; you want to confirm the stack came up.

**Do this:** nothing, usually — `bot-council` and `sovren-cloudflared` are
systemd units that start on boot. Check in this order:
1. `https://lqcouncil.com/api/health` in the browser — if ok, done.
2. If not, on EVO: `systemctl status bot-council` then
   `systemctl status sovren-cloudflared` — restart whichever is not
   "active (running)" with `sudo systemctl restart <name>` `[manual]`.

**It worked if:** the health URL is green and the journal shows
`service_started`.

**If not, hand an agent:** "EVO rebooted and the stack did not come back.
Status outputs: <paste both systemctl outputs>."
