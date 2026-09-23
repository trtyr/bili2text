import { useEffect, useState } from 'react'

interface ToolInfo {
  id: string
  name: string
  description: string
}

export default function App() {
  const [health, setHealth] = useState<'ok' | 'down' | 'checking'>('checking')
  const [tools, setTools] = useState<ToolInfo[]>([])

  useEffect(() => {
    fetch('/api/health')
      .then((r) => r.json())
      .then((d) => setHealth(d.status === 'ok' ? 'ok' : 'down'))
      .catch(() => setHealth('down'))
    fetch('/api/tools')
      .then((r) => r.json())
      .then(setTools)
      .catch(() => setTools([]))
  }, [])

  return (
    <main className="shell">
      <header className="header">
        <h1>🧰 Toolbox</h1>
        <span className={`dot dot-${health}`}>
          {health === 'ok' ? '服务在线' : health === 'checking' ? '连接中…' : '服务离线'}
        </span>
      </header>

      <section className="grid">
        {tools.map((t) => (
          <article key={t.id} className="card">
            <h2>{t.name}</h2>
            <p>{t.description}</p>
            <footer>
              <code>/api/tools/{t.id}</code>
              <button disabled title="骨架阶段，功能接入中">打开</button>
            </footer>
          </article>
        ))}
        {tools.length === 0 && <p className="empty">工具列表为空，快去注册一个吧</p>}
      </section>
    </main>
  )
}
