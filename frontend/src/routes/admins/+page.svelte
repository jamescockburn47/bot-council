<script lang="ts">
  import { api, ApiError } from '$lib/api/client';
  import { me } from '$lib/stores/me';
  import { goto } from '$app/navigation';
  import type { AdminEntry, SeenUserEntry } from '$lib/types';

  let admins = $state<AdminEntry[]>([]);
  let users = $state<SeenUserEntry[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let actionLoading = $state<string | null>(null);

  // Non-admins get redirected.
  $effect(() => {
    if ($me && $me.role !== 'admin') {
      goto('/');
    }
  });

  function formatDate(iso: string): string {
    return new Date(iso).toLocaleString('en-GB', {
      day: 'numeric', month: 'short', year: 'numeric',
      hour: '2-digit', minute: '2-digit',
    });
  }

  async function load() {
    loading = true;
    error = null;
    try {
      const [a, u] = await Promise.all([api.admins.list(), api.users.list()]);
      admins = a;
      users = u;
    } catch (e) {
      error = e instanceof ApiError ? `Error ${e.status}` : 'Failed to load';
    } finally {
      loading = false;
    }
  }

  async function promote(user_id: string) {
    actionLoading = user_id;
    try {
      await api.admins.add(user_id);
      await load();
    } catch (e) {
      error = e instanceof ApiError ? `Error ${e.status}: ${JSON.stringify(e.body)}` : 'Promote failed';
    } finally {
      actionLoading = null;
    }
  }

  async function demote(user_id: string) {
    actionLoading = user_id;
    try {
      await api.admins.remove(user_id);
      await load();
    } catch (e) {
      error = e instanceof ApiError ? `Error ${e.status}: ${JSON.stringify(e.body)}` : 'Demote failed';
    } finally {
      actionLoading = null;
    }
  }

  // Users not yet promoted, sorted by last_seen_at descending.
  let promotable = $derived(users.filter(u => !u.is_admin));

  $effect(() => { load(); });
</script>

<style>
  .btn-demote:hover {
    border-color: rgba(239, 68, 68, 0.5) !important;
    color: #FCA5A5 !important;
  }
</style>

<div style="max-width: 56rem;">
  <!-- Header -->
  <div style="margin-bottom: 2rem;">
    <p class="tm-eyebrow" style="color: var(--indigo-400); margin-bottom: 6px;">Workspace · Roles</p>
    <div style="display: flex; align-items: baseline; gap: 12px;">
      <h1 style="font-family: var(--sans-product); font-weight: 800; font-size: 32px; color: var(--glow-txt); margin: 0;">
        Admins
      </h1>
      {#if !loading && !error}
        <span class="stat-serif" style="font-size: 22px;">{admins.length}</span>
      {/if}
    </div>
  </div>

  {#if loading}
    <div style="display: flex; flex-direction: column; gap: 12px;">
      {#each Array(3) as _}
        <div class="card-term" style="padding: 16px; animation: pulse 1.5s ease-in-out infinite;">
          <div style="height: 16px; background: var(--night-edge); border-radius: 4px; width: 33%;"></div>
        </div>
      {/each}
    </div>
  {:else if error}
    <div class="card-term" style="padding: 24px; border-color: rgba(239,68,68,0.3);">
      <p style="font-family: var(--mono-product); font-size: 13px; color: #FCA5A5; margin: 0 0 12px;">{error}</p>
      <button
        onclick={load}
        class="btn-dark-ghost"
        style="font-size: 12px; color: #FCA5A5;"
      >
        Retry
      </button>
    </div>
  {:else}
    <!-- Current admins -->
    <section style="margin-bottom: 2rem;">
      <p class="mono-label" style="margin-bottom: 12px;">Current admins ({admins.length})</p>
      {#if admins.length === 0}
        <div class="card-term" style="padding: 24px; text-align: center; font-family: var(--sans-product); font-size: 14px; color: var(--glow-mute);">
          No admins yet. Use the admin bearer token to promote your first admin — see the deploy runbook.
        </div>
      {:else}
        <div class="card-term" style="overflow: hidden; padding: 0;">
          <table style="width: 100%; border-collapse: collapse; font-size: 13px;">
            <thead>
              <tr style="border-bottom: 1px solid var(--night-rule2);">
                <th class="mono-label" style="text-align: left; padding: 10px 16px;">User ID</th>
                <th class="mono-label" style="text-align: left; padding: 10px 16px;">Granted</th>
                <th class="mono-label" style="text-align: left; padding: 10px 16px;">By</th>
                <th class="mono-label" style="text-align: right; padding: 10px 16px;">Action</th>
              </tr>
            </thead>
            <tbody>
              {#each admins as a (a.user_id)}
                <tr style="border-bottom: 1px solid var(--night-rule2);" class="card-term-hover">
                  <td style="padding: 10px 16px; font-family: var(--mono-product); font-size: 13px; color: var(--glow-txt);">
                    {a.user_id}
                    {#if a.user_id === $me?.user_id}
                      <span style="margin-left: 8px; font-size: 10px; color: var(--indigo-400);">(you)</span>
                    {/if}
                  </td>
                  <td style="padding: 10px 16px; font-family: var(--sans-product); font-size: 13px; color: var(--glow-mute);">{formatDate(a.granted_at)}</td>
                  <td style="padding: 10px 16px; font-family: var(--mono-product); font-size: 13px; color: var(--glow-mute);">{a.granted_by ?? '—'}</td>
                  <td style="padding: 10px 16px; text-align: right;">
                    <button
                      onclick={() => demote(a.user_id)}
                      disabled={actionLoading === a.user_id || a.user_id === $me?.user_id}
                      title={a.user_id === $me?.user_id ? "Can't demote yourself" : 'Demote'}
                      class="btn-dark-ghost btn-demote"
                      style="font-size: 12px; transition: border-color var(--dur-fast) var(--ease-standard), color var(--dur-fast) var(--ease-standard);"
                    >
                      {actionLoading === a.user_id ? '…' : 'Demote'}
                    </button>
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    </section>

    <!-- Promotable users -->
    <section>
      <p class="mono-label" style="margin-bottom: 8px;">Signed-in users ({promotable.length})</p>
      <p style="font-family: var(--sans-product); font-size: 13px; color: var(--glow-mute); margin-bottom: 12px;">
        Users appear here after they sign in at least once.
      </p>
      {#if promotable.length === 0}
        <div class="card-term" style="padding: 24px; text-align: center; font-family: var(--sans-product); font-size: 14px; color: var(--glow-mute);">
          No non-admin users have signed in yet.
        </div>
      {:else}
        <div class="card-term" style="overflow: hidden; padding: 0;">
          <table style="width: 100%; border-collapse: collapse; font-size: 13px;">
            <thead>
              <tr style="border-bottom: 1px solid var(--night-rule2);">
                <th class="mono-label" style="text-align: left; padding: 10px 16px;">User ID</th>
                <th class="mono-label" style="text-align: left; padding: 10px 16px;">First seen</th>
                <th class="mono-label" style="text-align: left; padding: 10px 16px;">Last seen</th>
                <th class="mono-label" style="text-align: right; padding: 10px 16px;">Action</th>
              </tr>
            </thead>
            <tbody>
              {#each promotable as u (u.user_id)}
                <tr style="border-bottom: 1px solid var(--night-rule2);" class="card-term-hover">
                  <td style="padding: 10px 16px; font-family: var(--mono-product); font-size: 13px; color: var(--glow-txt);">{u.user_id}</td>
                  <td style="padding: 10px 16px; font-family: var(--sans-product); font-size: 13px; color: var(--glow-mute);">{formatDate(u.first_seen_at)}</td>
                  <td style="padding: 10px 16px; font-family: var(--sans-product); font-size: 13px; color: var(--glow-mute);">{formatDate(u.last_seen_at)}</td>
                  <td style="padding: 10px 16px; text-align: right;">
                    <button
                      onclick={() => promote(u.user_id)}
                      disabled={actionLoading === u.user_id}
                      class="btn-indigo"
                      style="font-size: 12px; padding: 5px 14px;"
                    >
                      {actionLoading === u.user_id ? '…' : 'Promote'}
                    </button>
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    </section>
  {/if}
</div>
