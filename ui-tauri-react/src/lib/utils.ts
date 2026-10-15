import { clsx, type ClassValue } from "clsx"
import { twMerge } from "tailwind-merge"

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs))
}

/** True when two flat objects hold the same keys with identical values. */
export function shallowEqual<T extends object>(a: T, b: T): boolean {
  if (a === b) return true
  const aKeys = Object.keys(a) as (keyof T)[]
  if (aKeys.length !== Object.keys(b).length) return false
  return aKeys.every((key) => Object.is(a[key], b[key]))
}
