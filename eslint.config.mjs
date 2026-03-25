import { nextConfig } from 'eslint-config-next';

/** @type {import('eslint').Linter.Config[]} */
const eslintConfig = [
  ...nextConfig,
  {
    rules: {
      // Warn on explicit any — not an error so existing code doesn't break CI
      '@typescript-eslint/no-explicit-any': 'warn',
      // Allow unused vars prefixed with _ (intentionally unused params)
      '@typescript-eslint/no-unused-vars': ['warn', { argsIgnorePattern: '^_' }],
    },
  },
];

export default eslintConfig;
