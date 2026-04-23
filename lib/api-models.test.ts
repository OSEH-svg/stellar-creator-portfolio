import { describe, it, expect } from 'vitest';
import {
  apiSuccess,
  apiFailure,
  paginatedData,
  isApiSuccess,
  isApiFailure,
  isValidationError,
} from './api-models';

describe('apiSuccess', () => {
  it('builds a success envelope', () => {
    const res = apiSuccess({ id: '1' });
    expect(res.success).toBe(true);
    expect(res.data).toEqual({ id: '1' });
    expect(res.error).toBeNull();
  });

  it('includes optional message', () => {
    const res = apiSuccess(42, 'Created');
    expect(res.message).toBe('Created');
  });

  it('omits message when not provided', () => {
    const res = apiSuccess(42);
    expect(res.message).toBeUndefined();
  });
});

describe('apiFailure', () => {
  it('builds a failure envelope', () => {
    const res = apiFailure('NOT_FOUND', 'Creator not found');
    expect(res.success).toBe(false);
    expect(res.data).toBeNull();
    expect(res.error.code).toBe('NOT_FOUND');
    expect(res.error.message).toBe('Creator not found');
  });

  it('includes fieldErrors when provided', () => {
    const res = apiFailure('VALIDATION_ERROR', 'Invalid', [
      { field: 'title', message: 'Required' },
    ]);
    expect(res.error.fieldErrors).toHaveLength(1);
    expect(res.error.fieldErrors![0].field).toBe('title');
  });

  it('omits fieldErrors when not provided', () => {
    const res = apiFailure('BAD_REQUEST', 'Bad');
    expect(res.error.fieldErrors).toBeUndefined();
  });
});

describe('paginatedData', () => {
  it('computes totalPages correctly', () => {
    const pd = paginatedData(['a', 'b', 'c'], 1, 2, 5);
    expect(pd.pagination.totalPages).toBe(3);
    expect(pd.items).toHaveLength(3);
  });

  it('handles exact division', () => {
    const pd = paginatedData([], 1, 10, 30);
    expect(pd.pagination.totalPages).toBe(3);
  });
});

describe('isApiSuccess / isApiFailure', () => {
  it('narrows success correctly', () => {
    const res = apiSuccess('ok');
    expect(isApiSuccess(res)).toBe(true);
    expect(isApiFailure(res)).toBe(false);
  });

  it('narrows failure correctly', () => {
    const res = apiFailure('UNAUTHORIZED', 'No token');
    expect(isApiFailure(res)).toBe(true);
    expect(isApiSuccess(res)).toBe(false);
  });
});

describe('isValidationError', () => {
  it('returns true for VALIDATION_ERROR with fieldErrors', () => {
    const res = apiFailure('VALIDATION_ERROR', 'Invalid', [{ field: 'x', message: 'y' }]);
    expect(isValidationError(res.error)).toBe(true);
  });

  it('returns false for other codes', () => {
    const res = apiFailure('NOT_FOUND', 'Missing');
    expect(isValidationError(res.error)).toBe(false);
  });

  it('returns false for VALIDATION_ERROR without fieldErrors', () => {
    const res = apiFailure('VALIDATION_ERROR', 'Invalid');
    expect(isValidationError(res.error)).toBe(false);
  });
});
