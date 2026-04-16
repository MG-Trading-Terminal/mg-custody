<script lang="ts">
	import { api } from '$lib/api';
	import { onMount } from 'svelte';

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

	// Invite modal
	let showInvite = $state(false);
	let inviteEmail = $state('');
	let inviteRole = $state('viewer');
	let inviting = $state(false);
	let inviteError = $state('');

	// Use workspace_id from the first workspace (simple single-tenant for now)
	let workspaceId = $state('');

	const ROLES = ['viewer', 'editor', 'signer', 'admin', 'owner'];
	const ROLE_COLORS: Record<string, string> = {
		owner: 'var(--red, #f85149)',
		admin: 'var(--yellow, #d29922)',
		signer: 'var(--accent, #58a6ff)',
		editor: 'var(--green, #3fb950)',
		viewer: 'var(--fg-muted, #8b949e)',
	};

	async function loadWorkspace() {
		try {
			const workspaces = await api.getJson<{ id: string }[]>('/v1/workspaces');
			if (workspaces.length > 0) {
				workspaceId = workspaces[0].id;
			}
		} catch {
			// If workspaces endpoint fails, try with nil UUID
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
		} finally {
			loading = false;
		}
	}

	async function inviteUser() {
		if (!inviteEmail) return;
		inviting = true;
		inviteError = '';
		try {
			await api.post(`/v1/workspaces/${workspaceId}/users`, {
				email: inviteEmail,
				role: inviteRole,
			});
			showInvite = false;
			inviteEmail = '';
			inviteRole = 'viewer';
			await loadUsers();
		} catch (e) {
			inviteError = e instanceof Error ? e.message : 'Failed to invite user';
		} finally {
			inviting = false;
		}
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
			await api.put(`/v1/workspaces/${workspaceId}/users/${userId}`, {
				enabled: !currentEnabled,
			});
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

	function fmtDate(s: string | null): string {
		if (!s) return 'Never';
		return new Date(s).toLocaleDateString('en-US', { month: 'short', day: 'numeric', year: 'numeric' });
	}
</script>

<h1>User Management</h1>

<div class="toolbar">
	<span class="count">{users.length} user{users.length !== 1 ? 's' : ''}</span>
	<button class="btn-primary" onclick={() => { showInvite = true; }}>+ Invite User</button>
</div>

{#if error}
	<div class="card err">{error}</div>
{/if}

{#if loading}
	<div class="card muted">Loading users...</div>
{:else}
	<div class="card">
		<div class="table-wrap"><table>
			<thead>
				<tr>
					<th>Email</th>
					<th>Role</th>
					<th>Status</th>
					<th>Last Login</th>
					<th>Joined</th>
					<th>Actions</th>
				</tr>
			</thead>
			<tbody>
				{#each users as user}
					<tr class:disabled={!user.enabled}>
						<td class="email-cell">{user.email}</td>
						<td>
							<select
								class="role-select"
								value={user.role}
								onchange={(e) => changeRole(user.id, (e.target as HTMLSelectElement).value)}
								style="color: {ROLE_COLORS[user.role] || 'inherit'}"
							>
								{#each ROLES as r}
									<option value={r}>{r}</option>
								{/each}
							</select>
						</td>
						<td>
							<button
								class="status-toggle"
								class:active={user.enabled}
								onclick={() => toggleUser(user.id, user.enabled)}
								title={user.enabled ? 'Click to disable' : 'Click to enable'}
							>
								{user.enabled ? 'Active' : 'Disabled'}
							</button>
						</td>
						<td class="muted">{fmtDate(user.last_login_at)}</td>
						<td class="muted">{fmtDate(user.created_at)}</td>
						<td>
							<button class="btn-danger-sm" onclick={() => deleteUser(user.id, user.email)} title="Remove user">
								&times;
							</button>
						</td>
					</tr>
				{/each}
				{#if users.length === 0}
					<tr><td colspan="6" class="muted" style="text-align:center;padding:2rem">No users yet. Invite someone to get started.</td></tr>
				{/if}
			</tbody>
		</table></div>
	</div>
{/if}

<!-- Invite modal -->
{#if showInvite}
	<div class="modal-backdrop" role="presentation" onclick={() => { showInvite = false; }}></div>
	<div class="modal">
		<h2>Invite User</h2>
		<form onsubmit={(e) => { e.preventDefault(); inviteUser(); }}>
			{#if inviteError}
				<p class="err-text">{inviteError}</p>
			{/if}
			<label>
				Email
				<input type="email" bind:value={inviteEmail} required placeholder="user@example.com" />
			</label>
			<label>
				Role
				<select bind:value={inviteRole}>
					{#each ROLES as r}
						<option value={r}>{r}</option>
					{/each}
				</select>
			</label>
			<div class="modal-actions">
				<button type="button" class="btn-secondary" onclick={() => { showInvite = false; }}>Cancel</button>
				<button type="submit" class="btn-primary" disabled={inviting}>
					{inviting ? 'Inviting...' : 'Send Invite'}
				</button>
			</div>
		</form>
	</div>
{/if}

<style>
	h1 { margin-top: 0; font-size: 1.5rem; }

	.toolbar {
		display: flex;
		justify-content: space-between;
		align-items: center;
		margin-bottom: 1rem;
	}

	.count { color: var(--fg-muted, #8b949e); font-size: 0.9rem; }

	.card {
		background: var(--card-bg, #161b22);
		border: 1px solid var(--border, #30363d);
		border-radius: 8px;
		padding: 1.2rem;
		margin-bottom: 1rem;
	}

	.err { color: var(--red, #f85149); border-color: rgba(248, 81, 73, 0.3); background: rgba(248, 81, 73, 0.1); }

	.table-wrap { overflow-x: auto; }
	table { width: 100%; border-collapse: collapse; font-size: 0.88rem; }
	th { text-align: left; padding: 0.5rem 0.75rem; color: var(--fg-muted); border-bottom: 1px solid var(--border); font-size: 0.8rem; text-transform: uppercase; letter-spacing: 0.04em; }
	td { padding: 0.5rem 0.75rem; border-bottom: 1px solid var(--border); vertical-align: middle; }

	.email-cell { font-weight: 500; }

	tr.disabled { opacity: 0.5; }

	.role-select {
		background: var(--hover-bg, #1c2128);
		border: 1px solid var(--border);
		border-radius: 4px;
		padding: 0.25rem 0.4rem;
		font-size: 0.82rem;
		font-weight: 600;
		text-transform: uppercase;
		cursor: pointer;
	}

	.status-toggle {
		background: none;
		border: 1px solid var(--border);
		border-radius: 4px;
		padding: 0.2rem 0.5rem;
		font-size: 0.78rem;
		cursor: pointer;
		color: var(--fg-muted);
	}

	.status-toggle.active { color: var(--green); border-color: var(--green); }

	.btn-primary {
		background: var(--accent, #58a6ff);
		color: #0f1117;
		border: none;
		border-radius: 6px;
		padding: 0.5rem 1rem;
		font-size: 0.85rem;
		font-weight: 600;
		cursor: pointer;
	}

	.btn-primary:disabled { opacity: 0.6; cursor: not-allowed; }

	.btn-secondary {
		background: var(--hover-bg);
		color: var(--fg);
		border: 1px solid var(--border);
		border-radius: 6px;
		padding: 0.5rem 1rem;
		font-size: 0.85rem;
		cursor: pointer;
	}

	.btn-danger-sm {
		background: none;
		border: 1px solid var(--border);
		border-radius: 4px;
		padding: 0.15rem 0.5rem;
		color: var(--fg-muted);
		cursor: pointer;
		font-size: 1rem;
		line-height: 1;
	}

	.btn-danger-sm:hover { color: var(--red); border-color: var(--red); }

	/* Modal */
	.modal-backdrop {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.6);
		z-index: 200;
	}

	.modal {
		position: fixed;
		top: 50%;
		left: 50%;
		transform: translate(-50%, -50%);
		background: var(--card-bg);
		border: 1px solid var(--border);
		border-radius: 8px;
		padding: 1.5rem;
		z-index: 201;
		width: 90%;
		max-width: 400px;
	}

	.modal h2 { margin: 0 0 1rem; font-size: 1.1rem; }

	.modal label {
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
		font-size: 0.85rem;
		color: var(--fg-muted);
		margin-bottom: 0.8rem;
	}

	.modal input, .modal select {
		background: var(--hover-bg);
		border: 1px solid var(--border);
		border-radius: 6px;
		color: var(--fg);
		padding: 0.5rem 0.75rem;
		font-size: 0.9rem;
	}

	.modal-actions { display: flex; gap: 0.75rem; justify-content: flex-end; margin-top: 1rem; }

	.err-text { color: var(--red); font-size: 0.85rem; margin: 0 0 0.5rem; }

	.muted { color: var(--fg-muted); font-size: 0.85rem; }
</style>
