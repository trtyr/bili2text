import { useEffect, useState } from 'react'
import App from './App'
import Bili2Text from './Bili2Text'

type View = { kind: 'home' } | { kind: 'tool'; id: string }

/** 平台路由：门户 = 展示系统；点进工具 = 进入该工具的完整子系统。 */
export default function Root() {
  const [view, setView] = useState<View>({ kind: 'home' })

  useEffect(() => {
    window.scrollTo(0, 0)
  }, [view])

  if (view.kind === 'tool' && view.id === 'bili2text') {
    return <Bili2Text onBack={() => setView({ kind: 'home' })} />
  }
  return <App onOpenTool={(id) => setView({ kind: 'tool', id })} />
}
