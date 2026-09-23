import { useEffect, useState } from 'react'
import Home, { LogoMark, ToolGlyph } from './Home'
import type { ToolInfo } from './types'
import Bili2Text from './Bili2Text'

type View = { kind: 'home' } | { kind: 'tool'; id: string }

const TOOL_ROUTES: Record<string, { title: string }> = {
  bili2text: { title: 'B站视频转文字' },
}

/**
 * 应用外壳：侧栏（平台导航）+ 主内容区。
 * 门户 = 工具面板；点进工具 = 主区切换为该工具的子系统工作台，外壳保持常驻。
 */
export default function Root() {
  const [view, setView] = useState<View>({ kind: 'home' })
  const [tools, setTools] = useState<ToolInfo[]>([])
  const [health, setHealth] = useState<'ok' | 'down' | 'checking'>('checking')

  useEffect(() => {
    fetch('/api/tools')
      .then((r) => r.json())
      .then(setTools)
      .catch(() => setTools([]))
    fetch('/api/health')
      .then((r) => r.json())
      .then((d) => setHealth(d.status === 'ok' ? 'ok' : 'down'))
      .catch(() => setHealth('down'))
  }, [])

  useEffect(() => {
    window.scrollTo(0, 0)
  }, [view])

  const activeTool = view.kind === 'tool' ? view.id : null

  return (
    <div className="shell">
      <aside className="sidebar">
        <div className="brand">
          <span className="brand-mark">
            <LogoMark />
          </span>
          <span className="brand-name">
            Toolbox
            <small>个人工具集</small>
          </span>
        </div>

        <nav className="nav">
          <span className="nav-group">平台</span>
          <button
            className={`nav-item ${view.kind === 'home' ? 'active' : ''}`}
            onClick={() => setView({ kind: 'home' })}
          >
            <svg width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
              <rect x="3.5" y="3.5" width="7.5" height="7.5" rx="2" />
              <rect x="13" y="3.5" width="7.5" height="7.5" rx="2" />
              <rect x="3.5" y="13" width="7.5" height="7.5" rx="2" />
              <rect x="13" y="13" width="7.5" height="7.5" rx="2" />
            </svg>
            工具面板
          </button>

          <span className="nav-group">子系统</span>
          {tools.map((t) => (
            <button
              key={t.id}
              className={`nav-item ${activeTool === t.id ? 'active' : ''}`}
              onClick={() => setView({ kind: 'tool', id: t.id })}
            >
              <ToolGlyph id={t.id} size={17} />
              {t.name}
            </button>
          ))}
          {tools.length === 0 && <span className="nav-item disabled">加载中…</span>}
        </nav>

        <div className="sidebar-foot">
          <span className={`beacon beacon-${health}`}>
            {health === 'ok' ? '服务运行中' : health === 'checking' ? '连接中…' : '服务离线'}
          </span>
          <span className="foot-note">本地自托管 · 数据不出本机</span>
        </div>
      </aside>

      <main className="main">
        {view.kind === 'home' ? (
          <Home onOpenTool={(id) => setView({ kind: 'tool', id })} />
        ) : (
          <>
            <nav className="crumb" aria-label="位置">
              <button className="crumb-link" onClick={() => setView({ kind: 'home' })}>
                工具面板
              </button>
              <span className="crumb-sep">/</span>
              <span className="crumb-here">{TOOL_ROUTES[view.id]?.title ?? view.id}</span>
            </nav>
            {view.id === 'bili2text' && <Bili2Text />}
          </>
        )}
      </main>
    </div>
  )
}
