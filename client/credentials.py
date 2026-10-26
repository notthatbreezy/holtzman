"""Secrets live in Windows Credential Manager, never settings or model messages."""
import os


def openai_key():
    from keyring.backends.Windows import WinVaultKeyring
    return os.environ.get("OPENAI_API_KEY") or WinVaultKeyring().get_password("HoltzmanVoice", "openai-api-key")


def save_openai_key(parent=None):
    import tkinter as tk
    from tkinter import ttk, messagebox
    from keyring.backends.Windows import WinVaultKeyring
    own = parent is None
    window = tk.Tk() if own else tk.Toplevel(parent)
    window.title("HOLT · OpenAI API key")
    window.geometry("570x200")
    frame = ttk.Frame(window, padding=18)
    frame.pack(fill="both", expand=True)
    ttk.Label(frame, text="Paste an OpenAI API key below. Input is hidden.").pack(anchor="w")
    ttk.Label(frame, text="Saved in Windows Credential Manager; never paste it into chat.").pack(anchor="w", pady=(4, 12))
    value = tk.StringVar()
    entry = ttk.Entry(frame, textvariable=value, show="•", width=65)
    entry.pack(fill="x")
    def save():
        key = value.get().strip()
        if len(key) < 20 or any(c.isspace() for c in key):
            messagebox.showerror("HOLT", "Enter the API key value only.", parent=window)
            return
        try:
            WinVaultKeyring().set_password("HoltzmanVoice", "openai-api-key", key)
        except Exception:
            messagebox.showerror("HOLT", "Could not save the Windows credential.", parent=window)
            return
        value.set("")
        window.destroy()
    ttk.Button(frame, text="Save key", command=save).pack(anchor="e", pady=12)
    window.after(100, entry.focus_set)
    window.after(150, window.deiconify)
    window.after(200, window.lift)
    if own:
        window.mainloop()


if __name__ == "__main__":
    save_openai_key()
