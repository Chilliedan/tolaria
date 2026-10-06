import { useEffect, useState, type ComponentType, type ReactNode } from 'react'
import type { EditorProps } from './Editor'
import { EditorStartupFallback } from './EditorStartupFallback'
import { markStartupPhase, waitForStartupPhase } from '../lib/startupPerformance'

let editorModulePromise: Promise<{ Editor: ComponentType<EditorProps> }> | null = null

function loadEditorModule(): Promise<{ Editor: ComponentType<EditorProps> }> {
  editorModulePromise ??= (() => {
    markStartupPhase('editor_module_requested')
    return import('./Editor').then((module) => {
      markStartupPhase('editor_module_loaded')
      return module
    })
  })()
  return editorModulePromise
}

function LoadedEditor(props: EditorProps & { Editor: ComponentType<EditorProps> }) {
  const { Editor, ...editorProps } = props
  useEffect(() => { markStartupPhase('editor_committed') }, [])
  return <Editor {...editorProps} />
}

function commitLoadedEditor(
  lifecycle: AbortController,
  setEditor: (value: ComponentType<EditorProps>) => void,
  Editor: ComponentType<EditorProps>,
): void {
  if (!lifecycle.signal.aborted) setEditor(Editor)
}

/**
 * The AI workspace surface is rendered here, beside whichever editor tree is
 * mounted, so it keeps one position while the lazy editor bundle replaces the
 * startup fallback. Rendering it inside each of them remounted it on load and
 * dropped its state, such as a prompt being typed.
 */
export type LazyEditorProps = EditorProps & {
  aiWorkspaceSurface?: ReactNode
}

export function LazyEditor(props: LazyEditorProps) {
  const { aiWorkspaceSurface, ...editorProps } = props
  const [Editor, setEditor] = useState<ComponentType<EditorProps> | null>(null)

  useEffect(() => {
    const lifecycle = new AbortController()
    void (async () => {
      if (!props.activeTabPath) await waitForStartupPhase('react_shell')
      if (lifecycle.signal.aborted) return
      const module = await loadEditorModule()
      commitLoadedEditor(lifecycle, (Editor) => { setEditor(() => Editor) }, module.Editor)
    })()
    return () => { lifecycle.abort() }
  }, [props.activeTabPath])

  return (
    <div className="editor-shell relative flex min-h-0 min-w-0">
      {Editor ? <LoadedEditor Editor={Editor} {...editorProps} /> : <EditorStartupFallback {...editorProps} />}
      {props.showAIChat ? aiWorkspaceSurface : null}
    </div>
  )
}
