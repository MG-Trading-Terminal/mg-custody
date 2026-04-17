/**
 * i18n — multi-language support for the MG Custody dashboard.
 *
 * Locale state is stored in a Svelte writable store so all components
 * react to locale changes without a page reload.
 *
 * Usage:
 *   import { t, setLocale, getLocale, locale } from '$lib/i18n';
 *   $: label = t('nav.portfolio');   // reactive in Svelte templates
 *   setLocale('ru');
 */

import { writable, get } from 'svelte/store';

export type Locale = 'en' | 'ru' | 'zh';

// ---------------------------------------------------------------------------
// Translation tables
// ---------------------------------------------------------------------------

const translations: Record<Locale, Record<string, string>> = {
  en: {
    // Navigation
    'nav.portfolio':    'Portfolio',
    'nav.deposit':      'Deposit',
    'nav.withdraw':     'Withdraw',
    'nav.transactions': 'Transactions',
    'nav.approvals':    'Approvals',
    'nav.profile':      'Profile',
    'nav.wallets':      'Wallets',
    'nav.policies':     'Policies',
    'nav.users':        'Users',
    'nav.addresses':    'Addresses',
    'nav.tokens':       'Tokens',
    'nav.nfts':         'NFTs',
    'nav.audit':        'Audit Log',
    'nav.webhooks':     'Webhooks',
    'nav.backup':       'Backup',
    'nav.derivation':   'Derivation',
    'nav.emergency':    'Emergency',
    'nav.settings':     'Settings',
    'nav.admin':        'Admin',

    // Portfolio page
    'portfolio.title':           'Portfolio',
    'portfolio.total':           'Total Balance',
    'portfolio.assets':          'Assets',
    'portfolio.activity':        'Recent Activity',
    'portfolio.no_balances':     'No balances yet. Make a deposit to get started.',
    'portfolio.no_activity':     'No recent activity.',
    'portfolio.deposit':         'Deposit',
    'portfolio.withdraw':        'Withdraw',
    'portfolio.approvals':       'Approvals',
    'portfolio.col_asset':       'Asset',
    'portfolio.col_balance':     'Balance',
    'portfolio.col_usd':         'USD Value',
    'portfolio.col_pct':         '% of Portfolio',

    // Deposit page
    'deposit.title':        'Deposit',
    'deposit.select_chain': 'Select Chain',
    'deposit.address':      'Deposit Address',
    'deposit.history':      'Deposit History',
    'deposit.qr_hint':      'Scan QR code or copy address below',
    'deposit.memo_hint':    'Include memo if required by the chain',

    // Withdraw page
    'withdraw.title':       'Withdraw',
    'withdraw.amount':      'Amount',
    'withdraw.destination': 'Destination Address',
    'withdraw.estimate':    'Estimate Fee',
    'withdraw.submit':      'Submit Withdrawal',
    'withdraw.asset':       'Asset',
    'withdraw.chain':       'Chain',
    'withdraw.fee':         'Network Fee',

    // Transactions
    'tx.title':      'Transactions',
    'tx.type':       'Type',
    'tx.asset':      'Asset',
    'tx.amount':     'Amount',
    'tx.status':     'Status',
    'tx.date':       'Date',
    'tx.hash':       'Transaction Hash',
    'tx.no_txs':     'No transactions yet.',

    // Approvals
    'approvals.title':    'Pending Approvals',
    'approvals.approve':  'Approve',
    'approvals.reject':   'Reject',
    'approvals.no_items': 'No pending approvals.',
    'approvals.amount':   'Amount',
    'approvals.to':       'To Address',
    'approvals.tier':     'Approval Tier',

    // Profile
    'profile.title':           'Profile',
    'profile.email':           'Email',
    'profile.role':            'Role',
    'profile.mfa':             'Two-Factor Authentication',
    'profile.mfa_enabled':     'Enabled',
    'profile.mfa_disabled':    'Disabled',
    'profile.change_password': 'Change Password',
    'profile.login_history':   'Login History',
    'profile.devices':         'Registered Devices',

    // Wallets (admin)
    'wallets.title':     'Wallet Management',
    'wallets.hot':       'Hot Wallet',
    'wallets.warm':      'Warm Wallet',
    'wallets.cold':      'Cold Wallet',
    'wallets.balance':   'Balance',
    'wallets.target':    'Target Allocation',
    'wallets.sweep':     'Trigger Sweep',

    // Policies (admin)
    'policies.title':   'Access Policies',
    'policies.add':     'Add Policy',
    'policies.edit':    'Edit',
    'policies.delete':  'Delete',
    'policies.name':    'Policy Name',
    'policies.effect':  'Effect',
    'policies.allow':   'Allow',
    'policies.deny':    'Deny',

    // Users (admin)
    'users.title':        'Users',
    'users.invite':       'Invite User',
    'users.email':        'Email',
    'users.role':         'Role',
    'users.status':       'Status',
    'users.active':       'Active',
    'users.disabled':     'Disabled',
    'users.last_login':   'Last Login',

    // Audit log (admin)
    'audit.title':    'Audit Log',
    'audit.event':    'Event',
    'audit.user':     'User',
    'audit.ip':       'IP Address',
    'audit.date':     'Date',
    'audit.no_items': 'No audit events.',

    // Settings (admin)
    'settings.title':        'Settings',
    'settings.chain_nodes':  'Chain Node URLs',
    'settings.webhooks':     'Webhook Configuration',
    'settings.limits':       'Withdrawal Limits',
    'settings.save':         'Save Changes',

    // Common
    'common.loading':   'Loading…',
    'common.error':     'Error',
    'common.copy':      'Copy',
    'common.copied':    'Copied',
    'common.back':      'Back',
    'common.cancel':    'Cancel',
    'common.confirm':   'Confirm',
    'common.submit':    'Submit',
    'common.save':      'Save',
    'common.edit':      'Edit',
    'common.delete':    'Delete',
    'common.close':     'Close',
    'common.view':      'View',
    'common.retry':     'Retry',
    'common.dismiss':   'Dismiss',
    'common.enable':    'Enable',
    'common.disable':   'Disable',
    'common.apply':     'Apply',
    'common.clear':     'Clear',
    'common.search':    'Search',
    'common.filter':    'Filter',
    'common.export':    'Export',
    'common.refresh':   'Refresh',
    'common.no_data':   'No data available',
    'common.unknown':   'Unknown',
    'common.active':    'Active',
    'common.inactive':  'Inactive',
    'common.total':     'Total',
    'common.pending':   'Pending',
    'common.failed':    'Failed',
    'common.success':   'Success',
    'common.warning':   'Warning',
    'common.all':       'All',
    'common.none':      'None',
    'common.yes':       'Yes',
    'common.no':        'No',

    // Auth screens
    'auth.sign_in':          'Sign in',
    'auth.sign_in_title':    'Sign in',
    'auth.sign_in_sub':      'Access your custody console.',
    'auth.sign_up':          'Create an account',
    'auth.sign_up_title':    'Create your account',
    'auth.sign_up_sub':      'Free. Takes under 30 seconds.',
    'auth.sign_out':         'Sign out',
    'auth.email':            'Email',
    'auth.password':         'Password',
    'auth.confirm_password': 'Confirm password',
    'auth.remember':         'Keep me signed in on this device',
    'auth.forgot':           'Forgot password?',
    'auth.reset_title':      'Reset password',
    'auth.reset_sub':        "Enter your email — we'll send a reset link.",
    'auth.reset_send':       'Send reset link',
    'auth.reset_sent':       'If an account exists, a reset link has been sent.',
    'auth.back_to_signin':   'Back to sign in',
    'auth.new_here':         'New here?',
    'auth.already_have':     'Already have an account?',
    'auth.totp_title':       'Two-factor authentication',
    'auth.totp_sub':         'Enter the 6-digit code from your authenticator app.',
    'auth.totp_code':        'Authentication code',
    'auth.verify':           'Verify & continue',
    'auth.use_different':    'Use a different account',
    'auth.password_min':     'Password must be at least 12 characters',
    'auth.passwords_mismatch': 'Passwords do not match',
    'auth.accept_terms':     'I understand MG Custody holds my deposit addresses\' private keys and agree to the operator\'s terms.',
    'auth.strength.weak':      'Weak',
    'auth.strength.fair':      'Fair',
    'auth.strength.strong':    'Strong',
    'auth.strength.excellent': 'Excellent',

    // Access denied
    'denied.title':   'Access restricted',
    'denied.text':    'This page is reserved for {role} users and above.',
    'denied.current': 'Your current role is {role}. Ask your workspace owner to escalate your access if you need it.',
    'denied.back':    'Back to Portfolio',

    // Topbar / layout
    'layout.env.production': 'Production',
    'layout.env.testnet':    'Testnet',
    'layout.env.sandbox':    'Sandbox',
    'layout.env.select':     'Environment',
    'layout.theme.dark':     'Dark theme',
    'layout.theme.light':    'Light theme',
    'layout.search.hint':    'Search & jump…',
    'layout.notifications':  'Notifications',
    'layout.help':           'Help',
    'layout.personal':       'Personal',
    'layout.organization':   'Organization',
    'layout.system':         'System',
    'layout.admin_badge':    'Admin',

    // Language picker
    'lang.en': 'English',
    'lang.ru': 'Russian',
    'lang.zh': 'Chinese',
  },

  ru: {
    // Navigation
    'nav.portfolio':    'Портфель',
    'nav.deposit':      'Пополнение',
    'nav.withdraw':     'Вывод',
    'nav.transactions': 'Транзакции',
    'nav.approvals':    'Подтверждения',
    'nav.profile':      'Профиль',
    'nav.wallets':      'Кошельки',
    'nav.policies':     'Политики',
    'nav.users':        'Пользователи',
    'nav.addresses':    'Адреса',
    'nav.tokens':       'Токены',
    'nav.nfts':         'NFT',
    'nav.audit':        'Журнал аудита',
    'nav.webhooks':     'Вебхуки',
    'nav.backup':       'Бэкап',
    'nav.derivation':   'Деривация',
    'nav.emergency':    'Экстренное',
    'nav.settings':     'Настройки',
    'nav.admin':        'Администрирование',

    // Portfolio page
    'portfolio.title':           'Портфель',
    'portfolio.total':           'Общий баланс',
    'portfolio.assets':          'Активы',
    'portfolio.activity':        'Последняя активность',
    'portfolio.no_balances':     'Балансов нет. Сделайте пополнение для начала работы.',
    'portfolio.no_activity':     'Нет последних операций.',
    'portfolio.deposit':         'Пополнить',
    'portfolio.withdraw':        'Вывести',
    'portfolio.approvals':       'Подтверждения',
    'portfolio.col_asset':       'Актив',
    'portfolio.col_balance':     'Баланс',
    'portfolio.col_usd':         'USD',
    'portfolio.col_pct':         '% портфеля',

    // Deposit page
    'deposit.title':        'Пополнение',
    'deposit.select_chain': 'Выберите сеть',
    'deposit.address':      'Адрес для пополнения',
    'deposit.history':      'История пополнений',
    'deposit.qr_hint':      'Отсканируйте QR-код или скопируйте адрес ниже',
    'deposit.memo_hint':    'Укажите мемо, если требуется сетью',

    // Withdraw page
    'withdraw.title':       'Вывод средств',
    'withdraw.amount':      'Сумма',
    'withdraw.destination': 'Адрес получателя',
    'withdraw.estimate':    'Рассчитать комиссию',
    'withdraw.submit':      'Отправить вывод',
    'withdraw.asset':       'Актив',
    'withdraw.chain':       'Сеть',
    'withdraw.fee':         'Комиссия сети',

    // Transactions
    'tx.title':      'Транзакции',
    'tx.type':       'Тип',
    'tx.asset':      'Актив',
    'tx.amount':     'Сумма',
    'tx.status':     'Статус',
    'tx.date':       'Дата',
    'tx.hash':       'Хеш транзакции',
    'tx.no_txs':     'Транзакций пока нет.',

    // Approvals
    'approvals.title':    'Ожидают подтверждения',
    'approvals.approve':  'Подтвердить',
    'approvals.reject':   'Отклонить',
    'approvals.no_items': 'Нет ожидающих подтверждений.',
    'approvals.amount':   'Сумма',
    'approvals.to':       'Адрес получателя',
    'approvals.tier':     'Уровень подтверждения',

    // Profile
    'profile.title':           'Профиль',
    'profile.email':           'Email',
    'profile.role':            'Роль',
    'profile.mfa':             'Двухфакторная аутентификация',
    'profile.mfa_enabled':     'Включена',
    'profile.mfa_disabled':    'Выключена',
    'profile.change_password': 'Изменить пароль',
    'profile.login_history':   'История входов',
    'profile.devices':         'Зарегистрированные устройства',

    // Wallets (admin)
    'wallets.title':     'Управление кошельками',
    'wallets.hot':       'Горячий кошелёк',
    'wallets.warm':      'Тёплый кошелёк',
    'wallets.cold':      'Холодный кошелёк',
    'wallets.balance':   'Баланс',
    'wallets.target':    'Целевое распределение',
    'wallets.sweep':     'Выполнить перемещение',

    // Policies (admin)
    'policies.title':   'Политики доступа',
    'policies.add':     'Добавить политику',
    'policies.edit':    'Редактировать',
    'policies.delete':  'Удалить',
    'policies.name':    'Название политики',
    'policies.effect':  'Эффект',
    'policies.allow':   'Разрешить',
    'policies.deny':    'Запретить',

    // Users (admin)
    'users.title':        'Пользователи',
    'users.invite':       'Пригласить пользователя',
    'users.email':        'Email',
    'users.role':         'Роль',
    'users.status':       'Статус',
    'users.active':       'Активен',
    'users.disabled':     'Отключён',
    'users.last_login':   'Последний вход',

    // Audit log (admin)
    'audit.title':    'Журнал аудита',
    'audit.event':    'Событие',
    'audit.user':     'Пользователь',
    'audit.ip':       'IP адрес',
    'audit.date':     'Дата',
    'audit.no_items': 'Событий аудита нет.',

    // Settings (admin)
    'settings.title':        'Настройки',
    'settings.chain_nodes':  'URL узлов блокчейна',
    'settings.webhooks':     'Настройка вебхуков',
    'settings.limits':       'Лимиты на вывод',
    'settings.save':         'Сохранить изменения',

    // Common
    'common.loading':   'Загрузка…',
    'common.error':     'Ошибка',
    'common.copy':      'Копировать',
    'common.copied':    'Скопировано',
    'common.back':      'Назад',
    'common.cancel':    'Отмена',
    'common.confirm':   'Подтвердить',
    'common.submit':    'Отправить',
    'common.save':      'Сохранить',
    'common.edit':      'Изменить',
    'common.delete':    'Удалить',
    'common.close':     'Закрыть',
    'common.view':      'Просмотр',
    'common.retry':     'Повторить',
    'common.dismiss':   'Скрыть',
    'common.enable':    'Включить',
    'common.disable':   'Отключить',
    'common.apply':     'Применить',
    'common.clear':     'Очистить',
    'common.search':    'Поиск',
    'common.filter':    'Фильтр',
    'common.export':    'Экспорт',
    'common.refresh':   'Обновить',
    'common.no_data':   'Нет данных',
    'common.unknown':   'Неизвестно',
    'common.active':    'Активен',
    'common.inactive':  'Неактивен',
    'common.total':     'Всего',
    'common.pending':   'В ожидании',
    'common.failed':    'Ошибка',
    'common.success':   'Успех',
    'common.warning':   'Предупреждение',
    'common.all':       'Все',
    'common.none':      'Нет',
    'common.yes':       'Да',
    'common.no':        'Нет',

    // Auth screens
    'auth.sign_in':          'Войти',
    'auth.sign_in_title':    'Вход',
    'auth.sign_in_sub':      'Доступ к консоли custody.',
    'auth.sign_up':          'Создать аккаунт',
    'auth.sign_up_title':    'Регистрация',
    'auth.sign_up_sub':      'Бесплатно. Занимает меньше минуты.',
    'auth.sign_out':         'Выйти',
    'auth.email':            'Email',
    'auth.password':         'Пароль',
    'auth.confirm_password': 'Подтверждение пароля',
    'auth.remember':         'Запомнить меня',
    'auth.forgot':           'Забыли пароль?',
    'auth.reset_title':      'Восстановление пароля',
    'auth.reset_sub':        'Введите email — мы отправим ссылку для сброса.',
    'auth.reset_send':       'Отправить ссылку',
    'auth.reset_sent':       'Если такой аккаунт существует — ссылка для сброса отправлена.',
    'auth.back_to_signin':   'Назад ко входу',
    'auth.new_here':         'Нет аккаунта?',
    'auth.already_have':     'Уже есть аккаунт?',
    'auth.totp_title':       'Двухфакторная аутентификация',
    'auth.totp_sub':         'Введите 6-значный код из приложения-аутентификатора.',
    'auth.totp_code':        'Код аутентификации',
    'auth.verify':           'Проверить и войти',
    'auth.use_different':    'Другой аккаунт',
    'auth.password_min':     'Пароль должен быть не меньше 12 символов',
    'auth.passwords_mismatch': 'Пароли не совпадают',
    'auth.accept_terms':     'Я понимаю, что MG Custody хранит приватные ключи моих депозитных адресов, и согласен с условиями оператора.',
    'auth.strength.weak':      'Слабый',
    'auth.strength.fair':      'Средний',
    'auth.strength.strong':    'Сильный',
    'auth.strength.excellent': 'Отличный',

    // Access denied
    'denied.title':   'Доступ ограничен',
    'denied.text':    'Эта страница доступна только пользователям с ролью {role} и выше.',
    'denied.current': 'Ваша роль — {role}. Попросите владельца воркспейса повысить вам доступ, если он необходим.',
    'denied.back':    'Вернуться к портфелю',

    // Topbar / layout
    'layout.env.production': 'Продакшен',
    'layout.env.testnet':    'Тестовая сеть',
    'layout.env.sandbox':    'Песочница',
    'layout.env.select':     'Окружение',
    'layout.theme.dark':     'Тёмная тема',
    'layout.theme.light':    'Светлая тема',
    'layout.search.hint':    'Поиск и навигация…',
    'layout.notifications':  'Уведомления',
    'layout.help':           'Помощь',
    'layout.personal':       'Личное',
    'layout.organization':   'Организация',
    'layout.system':         'Система',
    'layout.admin_badge':    'Admin',

    // Language picker
    'lang.en': 'Английский',
    'lang.ru': 'Русский',
    'lang.zh': 'Китайский',
  },

  zh: {
    // Navigation
    'nav.portfolio':    '资产组合',
    'nav.deposit':      '充值',
    'nav.withdraw':     '提现',
    'nav.transactions': '交易记录',
    'nav.approvals':    '审批',
    'nav.profile':      '个人资料',
    'nav.wallets':      '钱包',
    'nav.policies':     '策略',
    'nav.users':        '用户',
    'nav.addresses':    '地址',
    'nav.tokens':       '代币',
    'nav.nfts':         'NFT',
    'nav.audit':        '审计日志',
    'nav.webhooks':     'Webhooks',
    'nav.backup':       '备份',
    'nav.derivation':   '派生路径',
    'nav.emergency':    '紧急控制',
    'nav.settings':     '设置',
    'nav.admin':        '管理',

    // Portfolio page
    'portfolio.title':           '资产组合',
    'portfolio.total':           '总余额',
    'portfolio.assets':          '资产',
    'portfolio.activity':        '最近活动',
    'portfolio.no_balances':     '暂无余额，请先充值。',
    'portfolio.no_activity':     '暂无最近活动。',
    'portfolio.deposit':         '充值',
    'portfolio.withdraw':        '提现',
    'portfolio.approvals':       '审批',
    'portfolio.col_asset':       '资产',
    'portfolio.col_balance':     '余额',
    'portfolio.col_usd':         '美元价值',
    'portfolio.col_pct':         '占比',

    // Deposit page
    'deposit.title':        '充值',
    'deposit.select_chain': '选择链',
    'deposit.address':      '充值地址',
    'deposit.history':      '充值历史',
    'deposit.qr_hint':      '扫描二维码或复制以下地址',
    'deposit.memo_hint':    '如链要求，请填写备注',

    // Withdraw page
    'withdraw.title':       '提现',
    'withdraw.amount':      '金额',
    'withdraw.destination': '目标地址',
    'withdraw.estimate':    '估算手续费',
    'withdraw.submit':      '提交提现',
    'withdraw.asset':       '资产',
    'withdraw.chain':       '链',
    'withdraw.fee':         '网络手续费',

    // Transactions
    'tx.title':      '交易记录',
    'tx.type':       '类型',
    'tx.asset':      '资产',
    'tx.amount':     '金额',
    'tx.status':     '状态',
    'tx.date':       '日期',
    'tx.hash':       '交易哈希',
    'tx.no_txs':     '暂无交易记录。',

    // Approvals
    'approvals.title':    '待审批',
    'approvals.approve':  '批准',
    'approvals.reject':   '拒绝',
    'approvals.no_items': '没有待审批项目。',
    'approvals.amount':   '金额',
    'approvals.to':       '目标地址',
    'approvals.tier':     '审批级别',

    // Profile
    'profile.title':           '个人资料',
    'profile.email':           '邮箱',
    'profile.role':            '角色',
    'profile.mfa':             '双因素认证',
    'profile.mfa_enabled':     '已启用',
    'profile.mfa_disabled':    '未启用',
    'profile.change_password': '修改密码',
    'profile.login_history':   '登录历史',
    'profile.devices':         '已注册设备',

    // Wallets (admin)
    'wallets.title':     '钱包管理',
    'wallets.hot':       '热钱包',
    'wallets.warm':      '温钱包',
    'wallets.cold':      '冷钱包',
    'wallets.balance':   '余额',
    'wallets.target':    '目标配比',
    'wallets.sweep':     '触发归集',

    // Policies (admin)
    'policies.title':   '访问策略',
    'policies.add':     '添加策略',
    'policies.edit':    '编辑',
    'policies.delete':  '删除',
    'policies.name':    '策略名称',
    'policies.effect':  '效果',
    'policies.allow':   '允许',
    'policies.deny':    '拒绝',

    // Users (admin)
    'users.title':        '用户',
    'users.invite':       '邀请用户',
    'users.email':        '邮箱',
    'users.role':         '角色',
    'users.status':       '状态',
    'users.active':       '活跃',
    'users.disabled':     '已禁用',
    'users.last_login':   '最后登录',

    // Audit log (admin)
    'audit.title':    '审计日志',
    'audit.event':    '事件',
    'audit.user':     '用户',
    'audit.ip':       'IP 地址',
    'audit.date':     '日期',
    'audit.no_items': '暂无审计事件。',

    // Settings (admin)
    'settings.title':        '设置',
    'settings.chain_nodes':  '节点 URL',
    'settings.webhooks':     'Webhook 配置',
    'settings.limits':       '提现限额',
    'settings.save':         '保存更改',

    // Common
    'common.loading':   '加载中…',
    'common.error':     '错误',
    'common.copy':      '复制',
    'common.copied':    '已复制',
    'common.back':      '返回',
    'common.cancel':    '取消',
    'common.confirm':   '确认',
    'common.submit':    '提交',
    'common.save':      '保存',
    'common.edit':      '编辑',
    'common.delete':    '删除',
    'common.close':     '关闭',
    'common.view':      '查看',
    'common.retry':     '重试',
    'common.dismiss':   '忽略',
    'common.enable':    '启用',
    'common.disable':   '停用',
    'common.apply':     '应用',
    'common.clear':     '清除',
    'common.search':    '搜索',
    'common.filter':    '筛选',
    'common.export':    '导出',
    'common.refresh':   '刷新',
    'common.no_data':   '暂无数据',
    'common.unknown':   '未知',
    'common.active':    '活跃',
    'common.inactive':  '非活跃',
    'common.total':     '合计',
    'common.pending':   '待处理',
    'common.failed':    '失败',
    'common.success':   '成功',
    'common.warning':   '警告',
    'common.all':       '全部',
    'common.none':      '无',
    'common.yes':       '是',
    'common.no':        '否',

    // Auth screens
    'auth.sign_in':          '登录',
    'auth.sign_in_title':    '登录',
    'auth.sign_in_sub':      '访问您的托管控制台。',
    'auth.sign_up':          '创建账户',
    'auth.sign_up_title':    '创建账户',
    'auth.sign_up_sub':      '免费，耗时不到 30 秒。',
    'auth.sign_out':         '退出登录',
    'auth.email':            '邮箱',
    'auth.password':         '密码',
    'auth.confirm_password': '确认密码',
    'auth.remember':         '在此设备上保持登录',
    'auth.forgot':           '忘记密码?',
    'auth.reset_title':      '重置密码',
    'auth.reset_sub':        '输入邮箱 — 我们将发送重置链接。',
    'auth.reset_send':       '发送重置链接',
    'auth.reset_sent':       '如该账户存在，我们已发送重置链接。',
    'auth.back_to_signin':   '返回登录',
    'auth.new_here':         '还没有账户?',
    'auth.already_have':     '已有账户?',
    'auth.totp_title':       '双因素认证',
    'auth.totp_sub':         '输入您认证器应用中的 6 位验证码。',
    'auth.totp_code':        '认证码',
    'auth.verify':           '验证并继续',
    'auth.use_different':    '使用其他账户',
    'auth.password_min':     '密码至少 12 个字符',
    'auth.passwords_mismatch': '密码不匹配',
    'auth.accept_terms':     '我理解 MG Custody 保管我的存款地址私钥，并同意运营方的服务条款。',
    'auth.strength.weak':      '弱',
    'auth.strength.fair':      '中等',
    'auth.strength.strong':    '强',
    'auth.strength.excellent': '极强',

    // Access denied
    'denied.title':   '访问受限',
    'denied.text':    '此页面仅供 {role} 及以上角色使用。',
    'denied.current': '您当前的角色为 {role}。如需访问，请联系工作区所有者。',
    'denied.back':    '返回资产组合',

    // Topbar / layout
    'layout.env.production': '生产',
    'layout.env.testnet':    '测试网',
    'layout.env.sandbox':    '沙盒',
    'layout.env.select':     '环境',
    'layout.theme.dark':     '深色主题',
    'layout.theme.light':    '浅色主题',
    'layout.search.hint':    '搜索与跳转…',
    'layout.notifications':  '通知',
    'layout.help':           '帮助',
    'layout.personal':       '个人',
    'layout.organization':   '组织',
    'layout.system':         '系统',
    'layout.admin_badge':    '管理员',

    // Language picker
    'lang.en': '英语',
    'lang.ru': '俄语',
    'lang.zh': '中文',
  },
};

// ---------------------------------------------------------------------------
// Locale store
// ---------------------------------------------------------------------------

const STORAGE_KEY = 'mg_custody_locale';

function detectInitialLocale(): Locale {
  if (typeof localStorage !== 'undefined') {
    const stored = localStorage.getItem(STORAGE_KEY) as Locale | null;
    if (stored && stored in translations) {
      return stored;
    }
  }
  // Browser language hint
  if (typeof navigator !== 'undefined') {
    const lang = navigator.language.toLowerCase();
    if (lang.startsWith('ru')) return 'ru';
    if (lang.startsWith('zh')) return 'zh';
  }
  return 'en';
}

/** Svelte writable store for the active locale. Subscribe to re-render on change. */
export const locale = writable<Locale>(detectInitialLocale());

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/**
 * Get the translation for `key` in the currently active locale.
 * Falls back to the English translation if the key is missing.
 * Returns the key itself if no translation exists in any locale.
 *
 * Optional `vars` object substitutes `{name}` placeholders in the template.
 */
export function t(key: string, vars?: Record<string, string | number>): string {
  const current = get(locale);
  const raw =
    translations[current][key] ??
    translations['en'][key] ??
    key;
  if (!vars) return raw;
  return raw.replace(/\{(\w+)\}/g, (_, k) => String(vars[k] ?? `{${k}}`));
}

/**
 * Change the active locale. Persists to localStorage.
 * All reactive `$locale` subscriptions will update.
 */
export function setLocale(next: Locale): void {
  locale.set(next);
  if (typeof localStorage !== 'undefined') {
    localStorage.setItem(STORAGE_KEY, next);
  }
}

/** Get the currently active locale string. */
export function getLocale(): Locale {
  return get(locale);
}

/** All available locales with display names in the target language. */
export const LOCALES: { code: Locale; label: string }[] = [
  { code: 'en', label: 'English' },
  { code: 'ru', label: 'Русский' },
  { code: 'zh', label: '中文' },
];
