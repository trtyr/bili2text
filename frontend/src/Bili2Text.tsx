import { useState } from 'react'

interface ExtractResult {
  bvid: string
  title: string
  duration_secs: number
  subtitle: { lan: string; lan_doc: string; is_ai: boolean }
  lines_count: number
  text: string
  srt: string
}

interface ExtractError {
  error: string
  message?: string
}

export default function Bili2Text({ onBack }: { onBack: () => void }) {
  const [input, setInput] = useState('')
  const [busy, setBusy] = useState(false)
  const [result, setResult] = useState<ExtractResult | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [copied, setCopied] = useState(false)

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
        const err = d as ExtractError
        setError(
          err.error === 'login_required'
            ? '需要先扫码登录 B 站（右上角）'
            : err.error === 'no_subtitle'
              ? err.message ?? '该视频没有可用字幕'
              : err.error === 'bad_input'
                ? '没能从这个输入里解析出 BV 号'
                : (err.message ?? '提取失败'),
        )
        return
      }
      setResult(d as ExtractResult)
    } catch {
      setError('网络错误，服务还在线吗？')
    } finally {
      setBusy(false)
    }
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

  const fmtDur = (s: number) => {
    const m = Math.floor(s / 60)
    const sec = s % 60
    return `${m}分${sec}秒`
  }

  return (
    <main className="shell">
      <header className="header">
        <button className="ghost" onClick={onBack}>
          ← 工具箱
        </button>
        <h1>B站视频转文字</h1>
      </header>

      <section className="panel">
        <div className="input-row">
          <input
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && extract()}
            placeholder="粘贴 B 站视频链接、BV 号或 b23.tv 短链"
            autoFocus
          />
          <button onClick={extract} disabled={busy || !input.trim()}>
            {busy ? '提取中…' : '提取字幕'}
          </button>
        </div>
        <p className="hint">
          优先提取视频自带字幕（CC / AI），多语言自动选中文；无字幕时提示。AI
          字幕需要登录。
        </p>
      </section>

      {error && (
        <section className="panel error-panel">
          <p>⚠️ {error}</p>
        </section>
      )}

      {result && (
        <section className="panel">
          <div className="result-head">
            <h2>{result.title}</h2>
            <span className="meta">
              {result.subtitle.lan_doc}
              {result.subtitle.is_ai ? '（AI 生成）' : ''} · {result.lines_count} 条 ·{' '}
              {fmtDur(result.duration_secs)}
            </span>
          </div>
          <textarea readOnly value={result.text} rows={14} />
          <div className="result-actions">
            <button onClick={copyText}>{copied ? '已复制 ✓' : '复制全文'}</button>
            <button onClick={downloadSrt}>下载 .srt 字幕</button>
          </div>
        </section>
      )}
    </main>
  )
}
