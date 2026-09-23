import { useState } from 'react'
import App from './App'
import Bili2Text from './Bili2Text'

type View = { kind: 'home' } | { kind: 'tool'; id: string }

export default function Root() {
  const [view, setView] = useState<View>({ kind: 'home' })

  if (view.kind === 'tool' && view.id === 'bili2text') {
    return <Bili2Text onBack={() => setView({ kind: 'home' })} />
  }
  return <App onOpenTool={(id) => setView({ kind: 'tool', id })} />
}
