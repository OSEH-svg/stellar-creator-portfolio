import { clsx, type ClassValue } from 'clsx'
import { twMerge } from 'tailwind-merge'

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs))
}

// ── Formatting utilities ──────────────────────────────────────────────────────

/**
 * Format a number as a currency string.
 * @example formatCurrency(3000) // "$3,000"
 * @example formatCurrency(1500, 'EUR') // "€1,500"
 */
export function formatCurrency(amount: number, currency = 'USD'): string {
  return new Intl.NumberFormat('en-US', {
    style: 'currency',
    currency,
    maximumFractionDigits: 0,
  }).format(amount);
}

/**
 * Format an ISO date string or Date object as a localised date.
 * @example formatDate('2025-08-12') // "Aug 12, 2025"
 */
export function formatDate(date: string | Date): string {
  return new Intl.DateTimeFormat('en-US', {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
  }).format(new Date(date));
}

/**
 * Return a human-readable relative time string.
 * @example formatRelativeDate(new Date(Date.now() - 2 * 86400000)) // "2 days ago"
 */
export function formatRelativeDate(date: string | Date): string {
  const diffMs = Date.now() - new Date(date).getTime();
  const diffDays = Math.floor(diffMs / 86_400_000);
  if (diffDays === 0) return 'Today';
  if (diffDays === 1) return 'Yesterday';
  if (diffDays < 7) return `${diffDays} days ago`;
  if (diffDays < 30) return `${Math.floor(diffDays / 7)} weeks ago`;
  if (diffDays < 365) return `${Math.floor(diffDays / 30)} months ago`;
  return `${Math.floor(diffDays / 365)} years ago`;
}

/**
 * Format a deadline Date as "X days left" or "Expired".
 */
export function formatDeadline(deadline: Date | string): string {
  const diffMs = new Date(deadline).getTime() - Date.now();
  const diffDays = Math.ceil(diffMs / 86_400_000);
  if (diffDays < 0) return 'Expired';
  if (diffDays === 0) return 'Due today';
  if (diffDays === 1) return '1 day left';
  return `${diffDays} days left`;
}

/**
 * Capitalise the first letter of a string.
 */
export function capitalise(str: string): string {
  return str.charAt(0).toUpperCase() + str.slice(1);
}
