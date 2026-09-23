import { useEffect, useState } from 'react'

interface ToolInfo {
  id: string
  name: string
  description: string
}

/** 工具图标（自绘 SVG，stroke 统一 1.6）。 */
function ToolGlyph({ id }: { id: string }) {
  const common = {
    width: 22,
    height: 22,
    viewBox: '0 0 24 24',
    fill: 'none',
    stroke: 'currentColor',
    strokeWidth: 1.6,
    strokeLinecap: 'round' as const,
    strokeLinejoin: 'round' as const,
  }
  if (id === 'bili2text') {
    // 字幕条 → 文本行
    return (
      <svg {...common} aria-hidden>
        <rect x="3" y="5" width="18" height="11" rx="2.5" />
        <path d="M6.5 12.5h5M14 12.5h3.5" />
        <path d="M5.5 19.5h13" />
      </svg>
    )
  }
  // 兜底：方块加对角
  return (
    <svg {...common} aria-hidden>
      <rect x="4" y="4" width="16" height="16" rx="3" />
      <path d="M9 15l6-6" />
    </svg>
  )
}

export default function App({ onOpenTool }: { onOpenTool: (id: string) => void }) {
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
    <div className="page">
      <header className="masthead">
        <span className="wordmark">Toolbox</span>
        <span className={`beacon beacon-${health}`} title={`后端${health === 'ok' ? '在线' : '离线'}`}>
          {health === 'ok' ? '运行中' : health === 'checking' ? '连接中' : '离线'}
        </span>
      </header>

      <section className="hero">
        <h1>
          工具箱。
          <br />
          自己用的小工具，慢慢攒。
        </h1>
      </section>

      <section className="tool-list" aria-label="工具列表">
        <div className="list-head">
          <span>工具</span>
          <span>{tools.length} 个</span>
        </div>

        {tools.map((t) => (
          <button key={t.id} className="tool-row" onClick={() => onOpenTool(t.id)}>
            <span className="tool-glyph">
              <ToolGlyph id={t.id} />
            </span>
            <span className="tool-body">
              <span className="tool-name">{t.name}</span>
              <span className="tool-desc">{t.description}</span>
            </span>
            <span className="tool-arrow" aria-hidden>
              →
            </span>
          </button>
        ))}

        {tools.length === 0 && (
          <p className="list-empty">
            {health === 'ok' ? '还没有注册任何工具。' : '连不上后端服务，稍后再试试。'}
          </p>
        )}
      </section>

      <footer className="colophon">本地自托管 · Rust + React · 数据不出这台机器</footer>
    </div>
  )
}
