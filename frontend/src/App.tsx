import { useCallback, useEffect, useState } from 'react'
import { QRCodeSVG } from 'qrcode.react'

export interface LoginStatus {
  stage: 'unknown' | 'logged_out' | 'logged_in'
}

interface QrSession {
  qrcode_key: string
  qr_content: string
}

/// 扫码登录弹窗：申请二维码 → 轮询状态 → 成功回调。
function LoginModal({ onClose, onSuccess }: { onClose: () => void; onSuccess: () => void }) {
  const [session, setSession] = useState<QrSession | null>(null)
  const [tip, setTip] = useState('正在生成二维码…')
  const [done, setDone] = useState(false)

  const newSession = useCallback(async () => {
    setSession(null)
    setTip('正在生成二维码…')
    const r = await fetch('/api/auth/bili/qrcode', { method: 'POST' })
    const d = await r.json()
    setSession({ qrcode_key: d.qrcode_key, qr_content: d.qr_content })
    setTip('请用哔哩哔哩 App 扫码')
  }, [])

  useEffect(() => {
    newSession()
  }, [newSession])

  useEffect(() => {
    if (!session || done) return
    const timer = setInterval(async () => {
      const r = await fetch(`/api/auth/bili/qrcode/poll?qrcode_key=${session.qrcode_key}`)
      const d = await r.json()
      if (d.status === 'success') {
        setDone(true)
        setTip('登录成功！')
        clearInterval(timer)
        setTimeout(onSuccess, 600)
      } else if (d.status === 'scanned') {
        setTip('已扫码，请在手机上确认')
      } else if (d.status === 'expired') {
        clearInterval(timer)
        newSession()
      }
    }, 1500)
    return () => clearInterval(timer)
  }, [session, done, newSession, onSuccess])

  return (
    <div className="modal-mask" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <h3>扫码登录哔哩哔哩</h3>
        {session && (
          <div className="qr-box">
            <QRCodeSVG value={session.qr_content} size={180} />
          </div>
        )}
        <p>{tip}</p>
        <button onClick={onClose}>关闭</button>
      </div>
    </div>
  )
}

interface ToolInfo {
  id: string
  name: string
  description: string
}

export default function App({ onOpenTool }: { onOpenTool: (id: string) => void }) {
  const [health, setHealth] = useState<'ok' | 'down' | 'checking'>('checking')
  const [tools, setTools] = useState<ToolInfo[]>([])
  const [login, setLogin] = useState<LoginStatus>({ stage: 'unknown' })
  const [showLogin, setShowLogin] = useState(false)

  useEffect(() => {
    fetch('/api/health')
      .then((r) => r.json())
      .then((d) => setHealth(d.status === 'ok' ? 'ok' : 'down'))
      .catch(() => setHealth('down'))
    fetch('/api/tools')
      .then((r) => r.json())
      .then(setTools)
      .catch(() => setTools([]))
    fetch('/api/auth/bili/status')
      .then((r) => r.json())
      .then((d) => setLogin({ stage: d.logged_in ? 'logged_in' : 'logged_out' }))
      .catch(() => setLogin({ stage: 'logged_out' }))
  }, [])

  const logout = async () => {
    await fetch('/api/auth/bili', { method: 'DELETE' })
    setLogin({ stage: 'logged_out' })
  }

  return (
    <main className="shell">
      <header className="header">
        <h1>🧰 Toolbox</h1>
        <span className={`dot dot-${health}`}>
          {health === 'ok' ? '服务在线' : health === 'checking' ? '连接中…' : '服务离线'}
        </span>
        <span className="spacer" />
        {login.stage === 'logged_in' ? (
          <button className="ghost" onClick={logout}>
            B站已登录 · 退出
          </button>
        ) : (
          <button className="ghost" onClick={() => setShowLogin(true)}>
            扫码登录 B站
          </button>
        )}
      </header>

      <section className="grid">
        {tools.map((t) => (
          <article key={t.id} className="card clickable" onClick={() => onOpenTool(t.id)}>
            <h2>{t.name}</h2>
            <p>{t.description}</p>
            <footer>
              <code>/api/tools/{t.id}</code>
              <button>打开 →</button>
            </footer>
          </article>
        ))}
        {tools.length === 0 && <p className="empty">工具列表为空，快去注册一个吧</p>}
      </section>

      {showLogin && (
        <LoginModal
          onClose={() => setShowLogin(false)}
          onSuccess={() => {
            setLogin({ stage: 'logged_in' })
            setShowLogin(false)
          }}
        />
      )}
    </main>
  )
}
