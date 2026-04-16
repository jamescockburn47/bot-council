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

<div class="max-w-4xl">
  <h1 class="mono text-2xl font-bold mb-8">Admin Management</h1>

  {#if loading}
    <div class="space-y-3">
      {#each Array(3) as _}
        <div class="bg-[var(--surface)] border border-[var(--border)] rounded-lg p-4 animate-pulse">
          <div class="h-4 bg-[var(--border)] rounded w-1/3"></div>
        </div>
      {/each}
    </div>
  {:else if error}
    <div class="bg-red-500/10 border border-red-500/30 rounded-lg p-6">
      <p class="text-red-400 mono text-sm">{error}</p>
      <button
        onclick={load}
        class="mt-3 px-4 py-1.5 text-xs mono text-red-400 border border-red-500/30 rounded hover:bg-red-500/10 transition-colors"
      >
        Retry
      </button>
    </div>
  {:else}
    <!-- Current admins -->
    <section class="mb-8">
      <h2 class="mono text-sm text-[var(--text-secondary)] uppercase tracking-wider mb-3">
        Current admins ({admins.length})
      </h2>
      {#if admins.length === 0}
        <div class="bg-[var(--surface)] border border-[var(--border)] rounded-lg p-6 text-center text-sm text-[var(--text-muted)]">
          No admins yet. Use the admin bearer token to promote your first admin —
          see the deploy runbook.
        </div>
      {:else}
        <div class="bg-[var(--surface)] border border-[var(--border)] rounded-lg overflow-hidden">
          <table class="w-full text-sm">
            <thead>
              <tr class="border-b border-[var(--border)]">
                <th class="text-left py-2 px-4 text-xs mono text-[var(--text-muted)] font-normal">User ID</th>
                <th class="text-left py-2 px-4 text-xs mono text-[var(--text-muted)] font-normal">Granted</th>
                <th class="text-left py-2 px-4 text-xs mono text-[var(--text-muted)] font-normal">By</th>
                <th class="text-right py-2 px-4 text-xs mono text-[var(--text-muted)] font-normal">Action</th>
              </tr>
            </thead>
            <tbody>
              {#each admins as a (a.user_id)}
                <tr class="border-b border-[var(--border)] last:border-0">
                  <td class="py-2 px-4 mono text-xs text-[var(--text-primary)]">
                    {a.user_id}
                    {#if a.user_id === $me?.user_id}
                      <span class="ml-2 text-[10px] text-[#8b5cf6]">(you)</span>
                    {/if}
                  </td>
                  <td class="py-2 px-4 text-xs text-[var(--text-muted)]">{formatDate(a.granted_at)}</td>
                  <td class="py-2 px-4 mono text-xs text-[var(--text-muted)]">{a.granted_by ?? '—'}</td>
                  <td class="py-2 px-4 text-right">
                    <button
                      onclick={() => demote(a.user_id)}
                      disabled={actionLoading === a.user_id || a.user_id === $me?.user_id}
                      title={a.user_id === $me?.user_id ? "Can't demote yourself" : 'Demote'}
                      class="px-3 py-1 text-xs mono text-amber-400 border border-amber-500/30 rounded hover:bg-amber-500/10 transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
                    >
                      {actionLoading === a.user_id ? '...' : 'Demote'}
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
      <h2 class="mono text-sm text-[var(--text-secondary)] uppercase tracking-wider mb-3">
        Signed-in users ({promotable.length})
      </h2>
      <p class="text-xs text-[var(--text-muted)] mb-3">
        Users appear here after they sign in at least once.
      </p>
      {#if promotable.length === 0}
        <div class="bg-[var(--surface)] border border-[var(--border)] rounded-lg p-6 text-center text-sm text-[var(--text-muted)]">
          No non-admin users have signed in yet.
        </div>
      {:else}
        <div class="bg-[var(--surface)] border border-[var(--border)] rounded-lg overflow-hidden">
          <table class="w-full text-sm">
            <thead>
              <tr class="border-b border-[var(--border)]">
                <th class="text-left py-2 px-4 text-xs mono text-[var(--text-muted)] font-normal">User ID</th>
                <th class="text-left py-2 px-4 text-xs mono text-[var(--text-muted)] font-normal">First seen</th>
                <th class="text-left py-2 px-4 text-xs mono text-[var(--text-muted)] font-normal">Last seen</th>
                <th class="text-right py-2 px-4 text-xs mono text-[var(--text-muted)] font-normal">Action</th>
              </tr>
            </thead>
            <tbody>
              {#each promotable as u (u.user_id)}
                <tr class="border-b border-[var(--border)] last:border-0">
                  <td class="py-2 px-4 mono text-xs text-[var(--text-primary)]">{u.user_id}</td>
                  <td class="py-2 px-4 text-xs text-[var(--text-muted)]">{formatDate(u.first_seen_at)}</td>
                  <td class="py-2 px-4 text-xs text-[var(--text-muted)]">{formatDate(u.last_seen_at)}</td>
                  <td class="py-2 px-4 text-right">
                    <button
                      onclick={() => promote(u.user_id)}
                      disabled={actionLoading === u.user_id}
                      class="px-3 py-1 text-xs mono text-green-400 border border-green-500/30 rounded hover:bg-green-500/10 transition-colors disabled:opacity-50"
                    >
                      {actionLoading === u.user_id ? '...' : 'Promote'}
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
