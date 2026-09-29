import { defineConfig, globalIgnores } from 'eslint/config';
import js from '@eslint/js';
import tseslint from 'typescript-eslint';
import svelte from 'eslint-plugin-svelte';
import globals from 'globals';

export default defineConfig(
  globalIgnores(['dist/', 'node_modules/', 'src-tauri/target/', 'src-tauri/gen/']),
  js.configs.recommended,
  ...tseslint.configs.recommended,
  ...svelte.configs['flat/recommended'],
  {
    files: ['**/*.svelte'],
    languageOptions: {
      parserOptions: {
        parser: tseslint.parser,
        extraFileExtensions: ['.svelte'],
        sourceType: 'module',
        ecmaVersion: 'latest'
      }
    }
  },
  {
    languageOptions: {
      globals: { ...globals.browser }
    },
    rules: {
      '@typescript-eslint/no-explicit-any': 'error'
    }
  }
);
