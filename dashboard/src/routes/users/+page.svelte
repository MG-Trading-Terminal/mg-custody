<script lang="ts">
	import { api } from '$lib/api';
	import { onMount } from 'svelte';
	import { icon } from '$lib/icons/ui';

	interface WorkspaceUser {
		id: string;
		email: string;
		role: string;
		enabled: boolean;
		created_at: string;
		last_login_at: string | null;
	}

	let users = $state<WorkspaceUser[]>([]);
	let loading = $state(true);
	let error = $state('');

	let showInvite = $state(false);
	let inviteEmail = $state('');
	let inviteRole = $state('viewer');
	let inviting = $state(false);
	let inviteError = $state('');

	let workspaceId = $state('');
	let searchQuery = $state('');
	let roleFilter = $state<string>('all');

	const ROLES = ['viewer','editor','signer','admin','owner'];

	async function loadWorkspace() {
		try {
			const workspaces = await api.getJson<{ id: string }[]>('/v1/workspaces');
			if (workspaces.length > 0) workspaceId = workspaces[0].id;
		} catch {
			workspaceId = '00000000-0000-0000-0000-000000000000';
		}
	}

	async function loadUsers() {
		if (!workspaceId) return;
		loading = true;
		error = '';
		try {
			users = await api.getJson<WorkspaceUser[]>(`/v1/workspaces/${workspaceId}/users`);
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load users';
		} finally { loading = false; }
	}

	async function inviteUser() {
		if (!inviteEmail) return;
		inviting = true;
		inviteError = '';
		try {
			await api.post(`/v1/workspaces/${workspaceId}/users`, { email: inviteEmail, role: inviteRole });
			showInvite = false;
			inviteEmail = '';
			inviteRole = 'viewer';
			await loadUsers();
		} catch (e) {
			inviteError = e instanceof Error ? e.message : 'Failed to invite user';
		} finally { inviting = false; }
	}

	async function changeRole(userId: string, newRole: string) {
		try {
			await api.put(`/v1/workspaces/${workspaceId}/users/${userId}`, { role: newRole });
			await loadUsers();
		} catch (e) {
			alert(e instanceof Error ? e.message : 'Failed to change role');
		}
	}

	async function toggleUser(userId: string, currentEnabled: boolean) {
		try {
			await api.put(`/v1/workspaces/${workspaceId}/users/${userId}`, { enabled: !currentEnabled });
			await loadUsers();
		} catch (e) {
			alert(e instanceof Error ? e.message : 'Failed to toggle user');
		}
	}

	async function deleteUser(userId: string, email: string) {
		if (!confirm(`Remove ${email} from this workspace?`)) return;
		try {
			await api.del(`/v1/workspaces/${workspaceId}/users/${userId}`);
			await loadUsers();
		} catch (e) {
			alert(e instanceof Error ? e.message : 'Failed to remove user');
		}
	}

	onMount(async () => {
		await loadWorkspace();
		await loadUsers();
	});

	function initials(email: string): string {
		const local = email.split('@')[0] ?? '';
		const parts = local.split(/[._-]/).filter(Boolean);
		if (parts.length >= 2) return (parts[0][0] + parts[1][0]).toUpperCase();
		return local.slice(0, 2).toUpperCase();
	}

	function fmtDate(s: string | null): string {
		if (!s) return 'Never';
		const d = new Date(s);
		const diff = Date.now() - d.getTime();
		const dd = Math.floor(diff / 86_400_000);
		if (dd === 0) return 'Today';
		if (dd === 1) return 'Yesterday';
		if (dd < 7) return `${dd} days ago`;
		return d.toLocaleDateString(undefined, { month: 'short', day: 'numeric', year: 'numeric' });
	}

	function roleTone(r: string): 'owner' | 'admin' | 'signer' | 'editor' | 'viewer' {
		return (r as any) ?? 'viewer';
	}

	let filtered = $derived(users.filter(u => {
		if (roleFilter !== 'all' && u.role !== roleFilter) return false;
		if (searchQuery && !u.email.toLowerCase().includes(searchQuery.toLowerCase())) return false;
		return true;
	}));

	let stats = $derived({
		total: users.length,
		active: users.filter(u => u.enabled).length,
		admins: users.filter(u => u.role === 'owner' || u.role === 'admin').length,
		signers: users.filter(u => u.role === 'signer').length,
	});
</script>

<!-- ═══ Head ═══ -->
<div class="page-head">
	<div>
		<h1>Team</h1>
		<p class="page-sub">Workspace members, roles, and permissions. Role hierarchy: Owner &gt; Admin &gt; Signer &gt; Editor &gt; Viewer.</p>
	</div>
	<button class="btn btn-primary" onclick={() => { showInvite = true; inviteError = ''; }}>
		<span>{@html icon('plus', 14)}</span>
		<span>Invite member</span>
	</button>
</div>

<section class="stat-strip">
	<div class="stat-tile"><span class="stat-label">Total</span><span class="stat-value mono">{stats.total}</span></div>
	<div class="stat-tile"><span class="stat-label">Active</span><span class="stat-value mono tone-ok">{stats.active}</span></div>
	<div class="stat-tile"><span class="stat-label">Admins</span><span class="stat-value mono">{stats.admins}</span></div>
	<div class="stat-tile"><span class="stat-label">Signers</span><span class="stat-value mono">{stats.signers}</span></div>
</section>

{#if error}
	<div class="alert danger">
		<span>{@html icon('alert-triangle', 16)}</span>
		<span>{error}</span>
		<button class="btn btn-ghost sm" onclick={loadUsers}>Retry</button>
	</div>
{/if}

<!-- ═══ Toolbar ═══ -->
<section class="filter-bar">
	<div class="search-wrap">
		<span class="search-icon">{@html icon('search', 14)}</span>
		<input type="text" bind:value={searchQuery} placeholder="Search by email…" />
	</div>
	<div class="role-filter">
		<button class="chip-btn" class:active={roleFilter === 'all'} onclick={() => roleFilter = 'all'}>All</button>
		{#each ROLES as r}
			<button class="chip-btn tone-{r}" class:active={roleFilter === r} onclick={() => roleFilter = r}>
				<span class="role-dot"></span>
				<span class="capitalize">{r}</span>
			</button>
		{/each}
	</div>
</section>

<!-- ═══ Table ═══ -->
<section class="panel">
	<header class="panel-head">
		<div>
			<h2>Members</h2>
			<p class="panel-sub">{filtered.length} of {users.length}</p>
		</div>
	</header>

	{#if loading}
		<div class="panel-body">
			<div class="skel-line"></div>
			<div class="skel-line"></div>
		</div>
	{:else if filtered.length === 0}
		<div class="panel-empty">
			<span class="empty-mark">{@html icon('users', 22)}</span>
			<div>
				<p class="empty-title">{users.length === 0 ? 'No team members yet' : 'No matches'}</p>
				<p class="muted">{users.length === 0 ? 'Invite colleagues to collaborate on custody operations.' : 'Adjust search or role filter.'}</p>
			</div>
		</div>
	{:else}
		<table class="data-table">
			<thead>
				<tr>
					<th>Member</th>
					<th>Role</th>
					<th>Status</th>
					<th class="num">Last active</th>
					<th class="num">Joined</th>
					<th></th>
				</tr>
			</thead>
			<tbody>
				{#each filtered as user}
					<tr class:disabled={!user.enabled}>
						<td>
							<div class="member-cell">
								<span class="avatar-sm">{initials(user.email)}</span>
								<div>
									<span class="email">{user.email}</span>
									<span class="muted tiny mono">{user.id.slice(0, 8)}</span>
								</div>
							</div>
						</td>
						<td>
							<select
								class="role-select tone-{roleTone(user.role)}"
								value={user.role}
								onchange={(e) => changeRole(user.id, (e.target as HTMLSelectElement).value)}
							>
								{#each ROLES as r}<option value={r}>{r}</option>{/each}
							</select>
						</td>
						<td>
							<button
								class="status-toggle"
								class:active={user.enabled}
								onclick={() => toggleUser(user.id, user.enabled)}
								title={user.enabled ? 'Click to disable' : 'Click to enable'}
							>
								<span class="status-dot"></span>
								{user.enabled ? 'Active' : 'Disabled'}
							</button>
						</td>
						<td class="num mono muted">{fmtDate(user.last_login_at)}</td>
						<td class="num mono muted">{fmtDate(user.created_at)}</td>
						<td>
							<button class="icon-btn danger" onclick={() => deleteUser(user.id, user.email)} aria-label="Remove">
								{@html icon('x', 14)}
							</button>
						</td>
					</tr>
				{/each}
			</tbody>
		</table>
	{/if}
</section>

<!-- ═══ Invite modal ═══ -->
{#if showInvite}
	<div class="overlay" role="presentation" onclick={() => showInvite = false} onkeydown={(e) => e.key === 'Escape' && (showInvite = false)}>
		<div class="modal" role="dialog" aria-modal="true" onclick={(e) => e.stopPropagation()} onkeydown={() => {}}>
			<header class="modal-head">
				<h3>Invite team member</h3>
				<button class="icon-btn" onclick={() => showInvite = false}>{@html icon('x', 16)}</button>
			</header>

			<form class="modal-body" onsubmit={(e) => { e.preventDefault(); inviteUser(); }}>
				{#if inviteError}
					<div class="alert danger inline">
						<span>{@html icon('alert-triangle', 14)}</span>
						<span>{inviteError}</span>
					</div>
				{/if}

				<div class="field">
					<label for="inv-email">Email</label>
					<input id="inv-email" type="email" bind:value={inviteEmail} required placeholder="colleague@company.com" />
				</div>

				<div class="field">
					<label>Role</label>
					<div class="role-picker">
						{#each ROLES as r}
							<button
								type="button"
								class="role-option tone-{r}"
								class:active={inviteRole === r}
								onclick={() => inviteRole = r}
							>
								<span class="role-dot"></span>
								<span class="capitalize">{r}</span>
							</button>
						{/each}
					</div>
					<span class="role-hint muted tiny">
						{inviteRole === 'viewer' ? 'Read-only access to vaults and transactions' :
						 inviteRole === 'editor' ? 'Can create addresses, modify settings (no signing)' :
						 inviteRole === 'signer' ? 'Can approve Tier 2+ withdrawals with FIDO2' :
						 inviteRole === 'admin' ? 'Full access including user management' :
						 'Complete control including role changes'}
					</span>
				</div>

				<div class="modal-actions">
					<button type="button" class="btn btn-ghost" onclick={() => showInvite = false}>Cancel</button>
					<button type="submit" class="btn btn-primary" disabled={inviting}>
						{#if inviting}<span class="spinner"></span><span>Sending…</span>{:else}<span>Send invite</span>{/if}
					</button>
				</div>
			</form>
		</div>
	</div>
{/if}

<style>
	.page-head {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: var(--sp-4);
		margin-bottom: var(--sp-5);
		flex-wrap: wrap;
	}
	h1 { margin: 0 0 4px; font-size: 1.5rem; font-weight: 650; letter-spacing: -0.022em; }
	.page-sub { margin: 0; color: var(--fg-muted); font-size: 0.88rem; max-width: 720px; }

	.stat-strip {
		display: grid;
		grid-template-columns: repeat(4, 1fr);
		gap: 1px;
		background: var(--border);
		border: 1px solid var(--border);
		border-radius: var(--r-lg);
		overflow: hidden;
		margin-bottom: var(--sp-4);
		box-shadow: var(--shadow-1);
	}
	.stat-tile { padding: var(--sp-4) var(--sp-5); background: var(--surface); display: flex; flex-direction: column; gap: 6px; }
	.stat-label { font-size: 0.68rem; font-weight: 600; text-transform: uppercase; letter-spacing: 0.1em; color: var(--fg-muted); }
	.stat-value { font-size: 1.4rem; font-weight: 650; color: var(--fg-strong); letter-spacing: -0.02em; line-height: 1; }
	.stat-value.tone-ok { color: var(--green); }

	.alert {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 10px 14px;
		border-radius: var(--r-md);
		font-size: 0.85rem;
		margin-bottom: var(--sp-4);
	}
	.alert.inline { padding: 8px 10px; font-size: 0.8rem; margin: 0; }
	.alert.danger { background: var(--red-soft); color: var(--red); border: 1px solid rgba(248, 113, 113, 0.25); }
	.alert .btn { margin-left: auto; }

	.filter-bar {
		display: flex;
		gap: var(--sp-3);
		margin-bottom: var(--sp-4);
		flex-wrap: wrap;
		align-items: center;
	}
	.search-wrap {
		position: relative;
		flex: 1;
		min-width: 220px;
		max-width: 340px;
	}
	.search-icon { position: absolute; left: 10px; top: 50%; transform: translateY(-50%); color: var(--fg-dim); display: inline-flex; pointer-events: none; }
	.search-wrap input {
		width: 100%;
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: var(--r-sm);
		color: var(--fg);
		font-size: 0.85rem;
		padding: 7px 12px 7px 32px;
		outline: none;
	}
	.search-wrap input:focus { border-color: var(--accent); box-shadow: var(--shadow-focus); }

	.role-filter { display: flex; gap: 6px; flex-wrap: wrap; }

	.chip-btn {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		padding: 5px 11px;
		font-size: 0.8rem;
		font-weight: 500;
		border: 1px solid var(--border);
		border-radius: var(--r-full);
		background: var(--surface);
		color: var(--fg-muted);
		cursor: pointer;
		text-transform: capitalize;
		transition: background 0.15s, color 0.15s, border-color 0.15s;
	}
	.chip-btn:hover { background: var(--hover-bg); color: var(--fg); }
	.chip-btn.active { background: var(--accent-soft); color: var(--fg-strong); border-color: var(--accent-line); }

	.role-dot {
		width: 7px; height: 7px;
		border-radius: 50%;
		background: currentColor;
		display: inline-block;
	}
	.tone-owner { color: var(--red); }
	.tone-admin { color: var(--yellow); }
	.tone-signer { color: var(--accent); }
	.tone-editor { color: var(--green); }
	.tone-viewer { color: var(--fg-muted); }

	.panel {
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: var(--r-lg);
		box-shadow: var(--shadow-1);
		margin-bottom: var(--sp-4);
		overflow: hidden;
	}
	.panel-head {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: var(--sp-3);
		padding: var(--sp-4) var(--sp-5);
		border-bottom: 1px solid var(--border);
	}
	.panel-head h2 { margin: 0 0 2px; font-size: 0.92rem; font-weight: 600; color: var(--fg-strong); }
	.panel-sub { margin: 0; font-size: 0.78rem; color: var(--fg-muted); }
	.panel-body { padding: var(--sp-4) var(--sp-5); }

	/* Buttons */
	.btn {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 8px 14px;
		font-size: 0.86rem;
		font-weight: 550;
		border-radius: var(--r-sm);
		border: 1px solid var(--border);
		background: var(--surface-2);
		color: var(--fg);
		cursor: pointer;
		transition: background 0.15s, border-color 0.15s;
	}
	.btn.sm { padding: 4px 8px; font-size: 0.76rem; }
	.btn:hover:not(:disabled) { background: var(--hover-bg); border-color: var(--border-strong); }
	.btn:disabled { opacity: 0.5; cursor: not-allowed; }
	.btn-primary { background: var(--accent); color: #0A0B0D; border-color: var(--accent); font-weight: 600; }
	.btn-primary:hover:not(:disabled) { background: var(--accent-strong); border-color: var(--accent-strong); color: #0A0B0D; }
	.btn-ghost { background: transparent; border-color: transparent; color: var(--fg-muted); }
	.btn-ghost:hover { background: var(--hover-bg); color: var(--fg); border-color: var(--border); }

	.icon-btn {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 28px; height: 28px;
		background: transparent;
		border: 1px solid transparent;
		border-radius: var(--r-sm);
		color: var(--fg-muted);
		cursor: pointer;
	}
	.icon-btn:hover { background: var(--hover-bg); color: var(--fg); border-color: var(--border); }
	.icon-btn.danger:hover { color: var(--red); background: var(--red-soft); border-color: rgba(248, 113, 113, 0.3); }

	.spinner { width: 13px; height: 13px; border: 2px solid #0A0B0D; border-top-color: transparent; border-radius: 50%; animation: spin 0.7s linear infinite; }
	@keyframes spin { to { transform: rotate(360deg); } }

	/* Table */
	.data-table { width: 100%; border-collapse: collapse; font-size: 0.85rem; }
	.data-table thead th {
		text-align: left;
		padding: var(--sp-3) var(--sp-5);
		font-size: 0.68rem;
		font-weight: 600;
		color: var(--fg-muted);
		text-transform: uppercase;
		letter-spacing: 0.08em;
		border-bottom: 1px solid var(--border);
		background: var(--surface-2);
	}
	.data-table tbody td { padding: var(--sp-3) var(--sp-5); border-bottom: 1px solid var(--border); }
	.data-table tbody tr:last-child td { border-bottom: none; }
	.data-table tbody tr:hover { background: var(--hover-bg); }
	.data-table th.num, .data-table td.num { text-align: right; }
	tr.disabled { opacity: 0.55; }

	.member-cell { display: flex; align-items: center; gap: 10px; }
	.avatar-sm {
		width: 30px; height: 30px;
		border-radius: 50%;
		background: linear-gradient(135deg, var(--accent) 0%, #3D6BE0 100%);
		color: #0A0B0D;
		display: flex;
		align-items: center;
		justify-content: center;
		font-size: 0.68rem;
		font-weight: 700;
		flex-shrink: 0;
	}
	.member-cell > div { display: flex; flex-direction: column; line-height: 1.1; gap: 2px; }
	.email { font-weight: 500; color: var(--fg-strong); }
	.tiny { font-size: 0.7rem; }
	.muted { color: var(--fg-muted); }
	.capitalize { text-transform: capitalize; }

	.role-select {
		background: var(--surface-2);
		border: 1px solid var(--border);
		border-radius: var(--r-sm);
		padding: 4px 8px;
		font-size: 0.78rem;
		font-weight: 600;
		text-transform: capitalize;
		cursor: pointer;
		outline: none;
		color: currentColor;
	}
	.role-select:focus { border-color: var(--accent); }

	.status-toggle {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		background: var(--surface-2);
		border: 1px solid var(--border);
		border-radius: var(--r-full);
		padding: 3px 10px;
		font-size: 0.74rem;
		font-weight: 500;
		color: var(--fg-muted);
		cursor: pointer;
		transition: border-color 0.15s;
	}
	.status-toggle:hover { border-color: var(--border-strong); }
	.status-toggle .status-dot { width: 5px; height: 5px; border-radius: 50%; background: var(--fg-dim); }
	.status-toggle.active { color: var(--green); border-color: rgba(52, 211, 153, 0.3); background: var(--green-soft); }
	.status-toggle.active .status-dot { background: var(--green); box-shadow: 0 0 0 2px rgba(52, 211, 153, 0.18); }

	/* Empty & skel */
	.panel-empty {
		display: flex;
		align-items: center;
		gap: var(--sp-4);
		padding: var(--sp-6);
	}
	.empty-mark {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 44px; height: 44px;
		background: var(--accent-soft);
		color: var(--accent);
		border-radius: var(--r-md);
		flex-shrink: 0;
	}
	.panel-empty > div { flex: 1; }
	.empty-title { margin: 0 0 2px; font-size: 0.92rem; font-weight: 600; color: var(--fg-strong); }
	.panel-empty p { margin: 0; font-size: 0.82rem; }

	.skel-line {
		height: 44px;
		background: linear-gradient(90deg, var(--surface-2) 0%, var(--surface-3) 50%, var(--surface-2) 100%);
		background-size: 200% 100%;
		animation: shimmer 1.6s infinite;
		border-radius: var(--r-sm);
		margin-bottom: 6px;
	}
	@keyframes shimmer { 0% { background-position: 200% 0; } 100% { background-position: -200% 0; } }

	/* Modal */
	.overlay {
		position: fixed; inset: 0;
		background: rgba(0, 0, 0, 0.55);
		backdrop-filter: blur(4px);
		display: flex;
		align-items: center;
		justify-content: center;
		z-index: 100;
	}
	.modal {
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: var(--r-lg);
		width: 460px;
		max-width: calc(100vw - 40px);
		display: flex;
		flex-direction: column;
		box-shadow: 0 20px 60px rgba(0, 0, 0, 0.5);
	}
	.modal-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: var(--sp-4) var(--sp-5);
		border-bottom: 1px solid var(--border);
	}
	.modal-head h3 { margin: 0; font-size: 1rem; font-weight: 600; color: var(--fg-strong); }
	.modal-body {
		padding: var(--sp-5);
		display: flex;
		flex-direction: column;
		gap: var(--sp-4);
	}
	.modal-actions {
		display: flex;
		justify-content: flex-end;
		gap: var(--sp-2);
		margin-top: var(--sp-2);
	}

	.field { display: flex; flex-direction: column; gap: 6px; }
	.field label {
		font-size: 0.72rem;
		font-weight: 600;
		color: var(--fg-muted);
		text-transform: uppercase;
		letter-spacing: 0.08em;
	}
	.field input[type="email"], .field input:not([type]) {
		background: var(--surface-2);
		border: 1px solid var(--border);
		border-radius: var(--r-md);
		color: var(--fg);
		font-size: 0.88rem;
		padding: 9px 12px;
		outline: none;
	}
	.field input:focus { border-color: var(--accent); box-shadow: var(--shadow-focus); }

	.role-picker {
		display: grid;
		grid-template-columns: repeat(5, 1fr);
		gap: 4px;
	}
	.role-option {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 3px;
		padding: 8px 4px;
		background: var(--surface-2);
		border: 1px solid var(--border);
		border-radius: var(--r-sm);
		color: var(--fg);
		cursor: pointer;
		font-size: 0.76rem;
		font-weight: 500;
	}
	.role-option.active { border-color: var(--accent); background: var(--accent-soft); }
	.role-option .role-dot { width: 6px; height: 6px; }

	.role-hint { margin-top: 2px; }

	@media (max-width: 900px) {
		.stat-strip { grid-template-columns: repeat(2, 1fr); }
		.role-picker { grid-template-columns: repeat(3, 1fr); }
	}
</style>
