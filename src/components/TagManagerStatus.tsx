import { Button } from '@/components/ui/button'
import { translate, type AppLocale } from '../lib/i18n'
import type { TagManagerApplyResult, TagManagerProgress } from '../hooks/useTagManager'

interface TagManagerConfirmProps {
  message: string
  destructive: boolean
  locale: AppLocale
  onCancel: () => void
  onApply: () => void
}

export function TagManagerConfirm({ message, destructive, locale, onCancel, onApply }: TagManagerConfirmProps) {
  return (
    <div className="flex flex-col gap-2 rounded-md border border-border bg-muted/40 p-3" data-testid="tag-manager-confirm">
      <p className="text-sm">{message}</p>
      <div className="flex justify-end gap-2">
        <Button variant="outline" size="sm" onClick={onCancel}>{translate(locale, 'common.cancel')}</Button>
        <Button variant={destructive ? 'destructive' : 'default'} size="sm" onClick={onApply}>
          {translate(locale, 'tagManager.apply')}
        </Button>
      </div>
    </div>
  )
}

export function TagManagerProgressLine({ progress, locale }: { progress: TagManagerProgress; locale: AppLocale }) {
  const key = progress.total === 1 ? 'tagManager.applyingOne' : 'tagManager.applying'
  return (
    <p className="text-sm text-muted-foreground" role="status">
      {translate(locale, key, { done: progress.done, total: progress.total })}
    </p>
  )
}

interface TagManagerResultProps {
  result: TagManagerApplyResult
  titleForPath: (path: string) => string
  locale: AppLocale
}

export function TagManagerResult({ result, titleForPath, locale }: TagManagerResultProps) {
  if (result.total === 0) {
    return <p className="text-sm text-muted-foreground" role="status">{translate(locale, 'tagManager.result.noop')}</p>
  }
  if (result.failedPaths.length === 0) {
    const successKey = result.changed === 1 ? 'tagManager.result.successOne' : 'tagManager.result.success'
    return <p className="text-sm" role="status">{translate(locale, successKey, { count: result.changed })}</p>
  }
  const partialKey = result.total === 1 ? 'tagManager.result.partialOne' : 'tagManager.result.partial'
  return (
    <div className="flex flex-col gap-1 text-sm" role="status">
      <p>
        {translate(locale, partialKey, {
          changed: result.changed, total: result.total, failed: result.failedPaths.length,
        })}
      </p>
      <ul className="list-disc pl-5 text-muted-foreground" data-testid="tag-manager-failed-list">
        {result.failedPaths.map((path) => <li key={path}>{titleForPath(path)}</li>)}
      </ul>
    </div>
  )
}
