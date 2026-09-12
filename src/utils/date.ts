import dayjs from 'dayjs'

export function toLocalInputValue(value = new Date()) {
  return dayjs(value).format('YYYY-MM-DDTHH:mm')
}

export function toLocalDateValue(value = new Date()) {
  return dayjs(value).format('YYYY-MM-DD')
}

export function formatDateTime(value?: string | null) {
  if (!value) return '-'
  const parsed = dayjs(value)
  return parsed.isValid() ? parsed.format('YYYY-MM-DD HH:mm') : value
}

export function formatBusinessDate(value?: string | null) {
  if (!value) return '-'
  const parsed = dayjs(value)
  return parsed.isValid() ? parsed.format('YYYY-MM-DD') : value
}

export function localDayIsoRange(value = new Date()) {
  const date = dayjs(value)
  return {
    start: date.startOf('day').toISOString(),
    end: date.endOf('day').toISOString(),
  }
}
