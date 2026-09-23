import { useCallback, useEffect, useRef, useState } from 'react'
import { QRCodeSVG } from 'qrcode.react'

interface QrSession {
  qrcode_key: string
  qr_content: string
}

/** 扫码登录弹窗：申请二维码 → 轮询状态 → 成功回调；失效自动换新码。 */
export function LoginModal({ onClose, onSuccess }: { onClose: () => void; onSuccess: () => void }) {
  const [session, setSession] = useState<QrSession | null>(null)
  const [tip, setTip] = useState('正在生成二维码…')
  const [done, setDone] = useState(false)

  const newSession = useCallback(async () => {
    setSession(null)
    setTip('正在生成二维码…')
    const r = await fetch('/api/platform/bili/qrcode', { method: 'POST' })
    const d = await r.json()
    setSession({ qrcode_key: d.qrcode_key, qr_content: d.qr_content })
    setTip('打开哔哩哔哩 App 扫一扫')
  }, [])

  useEffect(() => {
    newSession()
  }, [newSession])

  useEffect(() => {
    if (!session || done) return
    const timer = setInterval(async () => {
      try {
        const r = await fetch(
          `/api/platform/bili/qrcode/poll?qrcode_key=${session.qrcode_key}`,
        )
        const d = await r.json()
        if (d.status === 'success') {
          setDone(true)
          setTip('登录成功')
          clearInterval(timer)
          setTimeout(onSuccess, 500)
        } else if (d.status === 'scanned') {
          setTip('已扫码，请在手机上确认')
        } else if (d.status === 'expired') {
          clearInterval(timer)
          newSession()
        }
      } catch {
        /* 网络抖动，下一轮再试 */
      }
    }, 1500)
    return () => clearInterval(timer)
  }, [session, done, newSession, onSuccess])

  const escRef = useRef(onClose)
  escRef.current = onClose
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === 'Escape' && escRef.current()
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [])

  return (
    <div className="modal-mask" onClick={onClose}>
      <div
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-label="扫码登录哔哩哔哩"
        onClick={(e) => e.stopPropagation()}
      >
        <h3>登录哔哩哔哩</h3>
        <p className="modal-sub">登录后可提取 AI 字幕；登录态只保存在本机</p>
        <div className="qr-box">
          {session ? (
            <QRCodeSVG value={session.qr_content} size={176} />
          ) : (
            <div className="qr-skeleton" aria-hidden />
          )}
        </div>
        <p className={done ? 'qr-tip ok' : 'qr-tip'}>{tip}</p>
        <button className="btn ghost" onClick={onClose}>
          关闭
        </button>
      </div>
    </div>
  )
}

interface ExtractResult {
  bvid: string
  title: string
  duration_secs: number
  subtitle: { lan: string; lan_doc: string; is_ai: boolean }
  lines_count: number
  text: string
  srt: string
}

interface TaskItem {
  id: string
  input: string
  title: string | null
  bvid: string | null
  status: string
  error: string | null
  lang: string | null
  lines_count: number | null
  duration_secs: number | null
  created_at: number
}

interface TaskDetail extends TaskItem {
  result_text: string | null
  result_srt: string | null
}

interface TaskFull extends TaskDetail {
  stage: string | null
  progress: number | null
}

function fmtTime(ts: number) {
  const d = new Date(ts * 1000)
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${d.getMonth() + 1}/${d.getDate()} ${pad(d.getHours())}:${pad(d.getMinutes())}`
}

export default function Bili2Text() {
  const [input, setInput] = useState('')
  const [busy, setBusy] = useState(false)
  const [result, setResult] = useState<ExtractResult | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [copied, setCopied] = useState(false)
  const [loggedIn, setLoggedIn] = useState<boolean | null>(null)
  const [showLogin, setShowLogin] = useState(false)
  const [history, setHistory] = useState<TaskItem[]>([])
  const [transcribing, setTranscribing] = useState<{ id: string; stage: string; progress: number } | null>(null)

  const refreshHistory = useCallback(async () => {
    try {
      const r = await fetch('/api/platform/tasks?tool_id=bili2text&limit=50')
      const d = await r.json()
      setHistory(d.tasks ?? [])
    } catch {
      setHistory([])
    }
  }, [])

  useEffect(() => {
    refreshHistory()
  }, [refreshHistory])

  const refreshStatus = useCallback(async () => {
    try {
      const r = await fetch('/api/platform/bili/status')
      const d = await r.json()
      setLoggedIn(Boolean(d.logged_in))
    } catch {
      setLoggedIn(false)
    }
  }, [])

  useEffect(() => {
    refreshStatus()
  }, [refreshStatus])

  const extract = async () => {
    if (!input.trim() || busy) return
    setBusy(true)
    setResult(null)
    setError(null)
    setCopied(false)
    try {
      const r = await fetch('/api/tools/bili2text/extract', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ input: input.trim() }),
      })
      const d = await r.json()
      if (!r.ok || d.error) {
        if (d.error === 'login_required') setError('需要登录 B 站，点击右上角「扫码登录」')
        else if (d.error === 'no_subtitle') setError(d.message ?? '该视频没有可用字幕')
        else if (d.error === 'bad_input') setError('没能从输入里解析出 BV 号，检查下链接？')
        else setError(d.message ?? '提取失败，稍后再试')
        return
      }
      setResult(d as ExtractResult)
      refreshHistory()
    } catch {
      setError('网络错误，后端服务可能没在跑')
    } finally {
      setBusy(false)
    }
  }

  /** 回看一条历史：拉详情，填充结果区。 */
  const openHistory = async (id: string) => {
    try {
      const r = await fetch(`/api/platform/tasks/${id}`)
      const d = await r.json()
      const t = d.task as TaskDetail
      if (!t || t.status !== 'succeeded' || !t.result_text) {
        setError(t?.error ? `该记录当时未成功：${t.error}` : '这条记录没有可回看的内容')
        return
      }
      setError(null)
      setResult({
        bvid: t.bvid ?? '',
        title: t.title ?? t.input,
        duration_secs: t.duration_secs ?? 0,
        subtitle: { lan: t.lang ?? '', lan_doc: t.lang ?? '', is_ai: (t.lang ?? '').startsWith('ai') },
        lines_count: t.lines_count ?? 0,
        text: t.result_text,
        srt: t.result_srt ?? '',
      })
      window.scrollTo({ top: 0, behavior: 'smooth' })
    } catch {
      setError('读取历史记录失败')
    }
  }

  const removeHistory = async (id: string) => {
    await fetch(`/api/platform/tasks/${id}`, { method: 'DELETE' })
    setHistory((h) => h.filter((t) => t.id !== id))
    if (transcribing?.id === id) setTranscribing(null)
  }

  /** 本地转写（长任务）：启动后轮询任务进度，完成即回看结果。 */
  const transcribe = async () => {
    if (!input.trim() || busy || transcribing) return
    setBusy(true)
    setResult(null)
    setError(null)
    setCopied(false)
    try {
      const r = await fetch('/api/tools/bili2text/transcribe', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ input: input.trim() }),
      })
      const d = await r.json()
      if (!r.ok || d.error) {
        setError(d.message ?? '无法开始转写')
        return
      }
      const taskId = d.task_id as string
      setTranscribing({ id: taskId, stage: 'queued', progress: 0 })
      // 轮询进度直至完成
      for (;;) {
        await new Promise((res) => setTimeout(res, 2000))
        const pr = await fetch(`/api/platform/tasks/${taskId}`)
        const pd = await pr.json()
        const t = pd.task as TaskFull | undefined
        if (!t) continue
        if (t.status === 'running' || t.status === 'pending') {
          setTranscribing({
            id: taskId,
            stage: t.stage ?? 'working',
            progress: t.progress ?? 0,
          })
          continue
        }
        if (t.status === 'succeeded' && t.result_text) {
          setResult({
            bvid: t.bvid ?? '',
            title: t.title ?? t.input,
            duration_secs: t.duration_secs ?? 0,
            subtitle: {
              lan: t.lang ?? '',
              lan_doc: '本地转写（SenseVoice）',
              is_ai: false,
            },
            lines_count: t.lines_count ?? 0,
            text: t.result_text,
            srt: t.result_srt ?? '',
          })
        } else {
          setError(`转写失败：${t.error ?? '未知原因'}`)
        }
        break
      }
    } catch {
      setError('网络错误，后端服务可能没在跑')
    } finally {
      setBusy(false)
      setTranscribing(null)
      refreshHistory()
    }
  }

  const logout = async () => {
    await fetch('/api/platform/bili', { method: 'DELETE' })
    setLoggedIn(false)
  }

  const downloadSrt = () => {
    if (!result) return
    const blob = new Blob([result.srt], { type: 'text/plain;charset=utf-8' })
    const a = document.createElement('a')
    a.href = URL.createObjectURL(blob)
    a.download = `${result.title}.srt`
    a.click()
    URL.revokeObjectURL(a.href)
  }

  const copyText = async () => {
    if (!result) return
    await navigator.clipboard.writeText(result.text)
    setCopied(true)
    setTimeout(() => setCopied(false), 1500)
  }

  const fmtDur = (s: number) => `${Math.floor(s / 60)}分${s % 60}秒`

const STAGE_ZH: Record<string, string> = {
  queued: '排队中',
  downloading: '下载音频',
  transcoding: '转码',
  transcribing: '本地转写',
  working: '处理中',
}

  return (
    <div className="content-page">
      <div className="page-heading">
        <div>
          <h1>B站视频转文字</h1>
          <p className="page-sub">贴入视频链接，提取官方或 AI 字幕，导出纯文本或 SRT。</p>
        </div>
        {loggedIn === null ? null : loggedIn ? (
          <button className="btn ghost" onClick={logout} title="点击退出登录">
            <span className="dot-inline ok" /> 已登录 · 退出
          </button>
        ) : (
          <button className="btn accent" onClick={() => setShowLogin(true)}>
            扫码登录 B站
          </button>
        )}
      </div>

      <section className="panel workbench">
        <label className="field-label" htmlFor="bili-input">
          视频地址
        </label>
        <div className="input-row">
          <input
            id="bili-input"
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && extract()}
            placeholder="https://www.bilibili.com/video/BV… 或 BV 号或 b23.tv 短链"
            autoFocus
          />
          <div className="btn-group">
            <button className="btn accent" onClick={extract} disabled={busy || !input.trim()}>
              {busy && !transcribing ? '提取中…' : '提取字幕'}
            </button>
            <button
              className="btn"
              onClick={transcribe}
              disabled={busy || !input.trim() || Boolean(transcribing)}
              title="下载音频后用本地 SenseVoice 模型转写，适合无字幕视频；耗时较长"
            >
              本地转写
            </button>
          </div>
        </div>
        <p className="hint">
          提取字幕：优先官方、其次 AI（需登录），自动选中文。本地转写：无字幕视频用
          SenseVoice 模型在本机转写，耗时较长。
        </p>
        {transcribing && (
          <div className="task-progress" role="status">
            <div className="progress-bar">
              <div className="progress-fill" style={{ width: `${transcribing.progress}%` }} />
            </div>
            <span className="progress-text">
              {STAGE_ZH[transcribing.stage] ?? transcribing.stage} · {transcribing.progress}%
            </span>
          </div>
        )}
      </section>

      {error && (
        <section className="panel error-panel" role="alert">
          <p>{error}</p>
        </section>
      )}

      {result && (
        <section className="panel result-panel">
          <div className="result-head">
            <h2>{result.title}</h2>
            <span className="meta">
              {result.subtitle.lan_doc}
              {result.subtitle.is_ai ? '（AI 生成）' : ''} · {result.lines_count} 条 ·{' '}
              {fmtDur(result.duration_secs)}
            </span>
          </div>
          <textarea readOnly value={result.text} rows={14} aria-label="提取结果" />
          <div className="result-actions">
            <button className="btn" onClick={copyText}>
              {copied ? '已复制 ✓' : '复制全文'}
            </button>
            <button className="btn" onClick={downloadSrt}>
              下载 .srt
            </button>
          </div>
        </section>
      )}

      {showLogin && (
        <LoginModal
          onClose={() => setShowLogin(false)}
          onSuccess={() => {
            setLoggedIn(true)
            setShowLogin(false)
          }}
        />
      )}

      <section className="panel history-panel">
        <div className="history-head">
          <h2>历史记录</h2>
          <span className="meta">{history.length} 条 · 存储于本机 data/tasks.db</span>
        </div>
        {history.length === 0 ? (
          <p className="hint">还没有提取记录，上面贴个视频链接试试。</p>
        ) : (
          <ul className="history-list">
            {history.map((t) => (
              <li key={t.id} className={`history-row ${t.status}`}>
                <button className="history-open" onClick={() => openHistory(t.id)}>
                  <span className={`h-status ${t.status}`} title={t.error ?? t.status} />
                  <span className="h-body">
                    <span className="h-title">{t.title ?? t.input}</span>
                    <span className="h-meta">
                      {fmtTime(t.created_at)}
                      {t.lines_count ? ` · ${t.lines_count} 条` : ''}
                      {t.duration_secs ? ` · ${fmtDur(t.duration_secs)}` : ''}
                      {t.status === 'failed' ? ` · ${t.error ?? '失败'}` : ''}
                    </span>
                  </span>
                </button>
                <button
                  className="btn plain h-del"
                  title="删除这条记录"
                  onClick={() => removeHistory(t.id)}
                >
                  ✕
                </button>
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  )
}
