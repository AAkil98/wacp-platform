import { useState } from "react";

interface LoginScreenProps {
  onLogin: (token: string) => void;
  error: string | null;
  loading: boolean;
}

export function LoginScreen({ onLogin, error, loading }: LoginScreenProps) {
  const [token, setToken] = useState("");

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (token.trim()) {
      onLogin(token.trim());
    }
  };

  return (
    <div className="flex items-center justify-center h-screen bg-zinc-950">
      <form
        onSubmit={handleSubmit}
        className="bg-zinc-900 rounded-lg p-8 w-96 shadow-xl"
      >
        <h1 className="text-xl font-semibold text-white mb-1">WACP Highway</h1>
        <p className="text-zinc-400 text-sm mb-6">
          Enter your authentication token to connect.
        </p>

        <label className="block text-sm text-zinc-300 mb-1">Token</label>
        <input
          type="password"
          value={token}
          onChange={(e) => setToken(e.target.value)}
          placeholder="Paste token here"
          className="w-full bg-zinc-800 border border-zinc-700 rounded px-3 py-2 text-sm text-white placeholder-zinc-500 focus:outline-none focus:border-blue-500 mb-4"
          disabled={loading}
          autoFocus
        />

        {error && (
          <p className="text-red-400 text-sm mb-4">{error}</p>
        )}

        <button
          type="submit"
          disabled={loading || !token.trim()}
          className="w-full bg-blue-600 hover:bg-blue-700 disabled:bg-zinc-700 disabled:text-zinc-500 text-white font-medium py-2 rounded text-sm transition-colors"
        >
          {loading ? "Connecting\u2026" : "Connect"}
        </button>
      </form>
    </div>
  );
}
