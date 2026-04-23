import { describe, it, expect, vi, beforeEach } from 'vitest';
import { apiFetch, fetchCreator, fetchCreators, fetchFreelancers, fetchFreelancer, ApiClientError } from './api-client';
import { apiSuccess, apiFailure } from './api-models';

function mockFetch(body: unknown, status = 200) {
  vi.stubGlobal(
    'fetch',
    vi.fn().mockResolvedValue({
      status,
      json: () => Promise.resolve(body),
    }),
  );
}

beforeEach(() => {
  vi.unstubAllGlobals();
});

describe('apiFetch', () => {
  it('unwraps a success envelope', async () => {
    mockFetch(apiSuccess({ id: 'alex-studio' }));
    const data = await apiFetch<{ id: string }>('/api/creators/alex-studio');
    expect(data.id).toBe('alex-studio');
  });

  it('throws ApiClientError on API failure', async () => {
    mockFetch(apiFailure('NOT_FOUND', 'Creator not found'), 404);
    await expect(apiFetch('/api/creators/missing')).rejects.toThrow(ApiClientError);
  });

  it('sets correct error code from API failure', async () => {
    mockFetch(apiFailure('UNAUTHORIZED', 'No token'), 401);
    try {
      await apiFetch('/api/bounties');
    } catch (e) {
      expect(e).toBeInstanceOf(ApiClientError);
      expect((e as ApiClientError).code).toBe('UNAUTHORIZED');
      expect((e as ApiClientError).status).toBe(401);
    }
  });

  it('throws network error when fetch rejects', async () => {
    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new Error('Failed to fetch')));
    await expect(apiFetch('/api/creators')).rejects.toThrow(ApiClientError);
    await expect(apiFetch('/api/creators')).rejects.toMatchObject({
      code: 'SERVICE_UNAVAILABLE',
    });
  });

  it('throws on malformed JSON response', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue({
        status: 200,
        json: () => Promise.reject(new SyntaxError('Unexpected token')),
      }),
    );
    await expect(apiFetch('/api/creators')).rejects.toMatchObject({
      code: 'INTERNAL_SERVER_ERROR',
    });
  });

  it('attaches Content-Type and Accept headers', async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      status: 200,
      json: () => Promise.resolve(apiSuccess({})),
    });
    vi.stubGlobal('fetch', fetchMock);
    await apiFetch('/api/health');
    const [, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect((init.headers as Record<string, string>)['Content-Type']).toBe('application/json');
    expect((init.headers as Record<string, string>)['Accept']).toBe('application/json');
  });
});

describe('fetchCreator', () => {
  it('calls the correct endpoint', async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      status: 200,
      json: () => Promise.resolve(apiSuccess({ id: 'alex-studio', name: 'Alex Chen' })),
    });
    vi.stubGlobal('fetch', fetchMock);
    const creator = await fetchCreator('alex-studio');
    expect(creator.name).toBe('Alex Chen');
    expect((fetchMock.mock.calls[0] as [string])[0]).toContain('/api/creators/alex-studio');
  });
});

describe('fetchCreators', () => {
  it('appends discipline query param', async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      status: 200,
      json: () => Promise.resolve(apiSuccess({ creators: [], total: 0 })),
    });
    vi.stubGlobal('fetch', fetchMock);
    await fetchCreators({ discipline: 'UI/UX Design' });
    expect((fetchMock.mock.calls[0] as [string])[0]).toContain('discipline=UI%2FUX+Design');
  });

  it('appends search query param', async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      status: 200,
      json: () => Promise.resolve(apiSuccess({ creators: [], total: 0 })),
    });
    vi.stubGlobal('fetch', fetchMock);
    await fetchCreators({ search: 'figma' });
    expect((fetchMock.mock.calls[0] as [string])[0]).toContain('search=figma');
  });
});

describe('ApiClientError', () => {
  it('fromApiError maps fields correctly', () => {
    const err = ApiClientError.fromApiError(
      { code: 'VALIDATION_ERROR', message: 'Invalid', fieldErrors: [{ field: 'title', message: 'Required' }] },
      422,
    );
    expect(err.code).toBe('VALIDATION_ERROR');
    expect(err.status).toBe(422);
    expect(err.fieldErrors).toHaveLength(1);
  });

  it('network() returns SERVICE_UNAVAILABLE', () => {
    const err = ApiClientError.network();
    expect(err.code).toBe('SERVICE_UNAVAILABLE');
  });
});

describe('fetchFreelancers', () => {
  it('calls /api/freelancers without params', async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      status: 200,
      json: () => Promise.resolve(apiSuccess({ freelancers: [], total: 0 })),
    });
    vi.stubGlobal('fetch', fetchMock);
    await fetchFreelancers();
    expect((fetchMock.mock.calls[0] as [string])[0]).toContain('/api/freelancers');
  });

  it('appends discipline param', async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      status: 200,
      json: () => Promise.resolve(apiSuccess({ freelancers: [], total: 0 })),
    });
    vi.stubGlobal('fetch', fetchMock);
    await fetchFreelancers({ discipline: 'Writing' });
    expect((fetchMock.mock.calls[0] as [string])[0]).toContain('discipline=Writing');
  });
});

describe('fetchFreelancer', () => {
  it('calls the correct endpoint', async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      status: 200,
      json: () => Promise.resolve(apiSuccess({ address: 'wallet-1', name: 'Jane' })),
    });
    vi.stubGlobal('fetch', fetchMock);
    const result = await fetchFreelancer('wallet-1') as { address: string };
    expect(result.address).toBe('wallet-1');
    expect((fetchMock.mock.calls[0] as [string])[0]).toContain('/api/freelancers/wallet-1');
  });
});
