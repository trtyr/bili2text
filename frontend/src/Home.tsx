import { useEffect, useState } from 'react'
import type { ToolInfo } from './types'

/** 工具图标（自绘 SVG，stroke 统一 1.7）。 */
export function ToolGlyph({ id, size = 24 }: { id: string; size?: number }) {
  const common = {
    width: size,
    height: size,
    viewBox: '0 0 24 24',
    fill: 'none',
    stroke: 'currentColor',
    strokeWidth: 1.7,
    strokeLinecap: 'round' as const,
    strokeLinejoin: 'round' as const,
  }
  if (id === 'bili2text') {
    return (
      <svg {...common} aria-hidden>
        <rect x="2.5" y="5" width="19" height="11.5" rx="3" />
        <path d="M6.5 12.8h5.2M14.5 12.8h3" />
        <path d="M5.5 19.5h13" />
      </svg>
    )
  }
  return (
    <svg {...common} aria-hidden>
      <rect x="4" y="4" width="16" height="16" rx="3.5" />
      <path d="M9 15l6-6" />
    </svg>
  )
}

/** 平台 logo：工具箱图形。 */
export function LogoMark({ size = 26 }: { size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.7"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      <rect x="3" y="8" width="18" height="12" rx="2.5" />
      <path d="M9 8V6.5A2.5 2.5 0 0 1 11.5 4h1A2.5 2.5 0 0 1 15 6.5V8" />
      <path d="M3 13h18" />
      <path d="M10.5 13v2.5h3V13" />
    </svg>
  )
}

export default function Home({ onOpenTool }: { onOpenTool: (id: string) => void }) {
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
    <div className="content-page">
      <div className="page-heading">
        <div>
          <h1>工具面板</h1>
          <p className="page-sub">登录态与任务数据全部留在本机，随取随用。</p>
        </div>
      </div>

      <h2 className="section-title">全部工具</h2>
      <div className="tool-grid">
        {tools.map((t) => (
          <button key={t.id} className="tool-card" onClick={() => onOpenTool(t.id)}>
            <div className="card-top">
              <span className="glyph-tile">
                <ToolGlyph id={t.id} />
              </span>
              <span className="card-state ok">可用</span>
            </div>
            <span className="card-name">{t.name}</span>
            <span className="card-desc">{t.description}</span>
            <div className="card-foot">
              <span className="tag">字幕提取</span>
              <span className="tag dim">本地转写 · 规划中</span>
              <span className="card-go" aria-hidden>
                打开 →
              </span>
            </div>
          </button>
        ))}
        {tools.length === 0 && (
          <div className="grid-empty">
            {health === 'ok' ? '还没有注册任何工具。' : '后端服务未响应，请检查服务是否启动。'}
          </div>
        )}
      </div>
    </div>
  )
}
