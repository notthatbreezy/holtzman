"""Small Windows control panel for the HOLT client and its local services."""
import argparse
import asyncio
import base64
import io
import json
import logging
import os
import queue
import sys
import threading
import time
import tkinter as tk
from tkinter import ttk, messagebox

import psutil

from client.state import ROOT, load_settings, save_settings
from client.services import Services, matches


class App:
    def __init__(self, window, autostart=False):
        self.window = window
        window.title("HOLT · Ship computer")
        window.geometry("920x780")
        window.minsize(740, 650)
        self.settings = load_settings()
        self.events = queue.Queue()
        self.worker = None
        self.engine = None
        self.stopped = threading.Event()
        self.closing = False
        self.photo = None
        self.state = tk.StringVar(value="Stopped")
        self.ship = tk.StringVar(value="Not connected")
        self.microphone_status = tk.StringVar(value="Microphone stopped")
        style = ttk.Style()
        style.theme_use("vista" if "vista" in style.theme_names() else "clam")
        frame = ttk.Frame(window, padding=18)
        frame.pack(fill="both", expand=True)
        ttk.Label(frame, text="HOLT", font=("Segoe UI", 25, "bold")).pack(anchor="w")
        ttk.Label(frame, text="Your ship computer · George voice · local inference").pack(anchor="w")
        status = ttk.LabelFrame(frame, text="Operation", padding=10)
        status.pack(fill="x", pady=12)
        ttk.Label(status, textvariable=self.state, font=("Segoe UI", 12, "bold")).pack(anchor="w")
        ttk.Label(status, textvariable=self.ship).pack(anchor="w", pady=(3, 8))
        ttk.Label(status, textvariable=self.microphone_status).pack(anchor="w")
        buttons = ttk.Frame(status)
        buttons.pack(fill="x")
        self.start_button = ttk.Button(buttons, text="Start HOLT", command=self.start)
        self.start_button.pack(side="left")
        self.stop_button = ttk.Button(buttons, text="Stop HOLT", command=self.stop, state="disabled")
        self.stop_button.pack(side="left", padx=8)
        ttk.Button(buttons, text="New conversation", command=self.new_session).pack(side="right")
        options = ttk.LabelFrame(frame, text="Audio and preferences · changes apply on next Start", padding=10)
        options.pack(fill="x")
        self.microphone = tk.StringVar(value=self.settings["microphone"] or "Windows default")
        self.speaker = tk.StringVar(value=self.settings["speaker"] or "Windows default")
        from client.audio import devices
        for row, (label, variable, kind) in enumerate([("Microphone", self.microphone, "input"), ("Speakers / headphones", self.speaker, "output")]):
            ttk.Label(options, text=label).grid(row=row, column=0, sticky="w", padx=(0, 10))
            combo = ttk.Combobox(options, textvariable=variable, state="readonly",
                                 values=["Windows default", *[name for name, _ in devices(kind)]])
            combo.grid(row=row, column=1, sticky="ew", pady=3)
        options.columnconfigure(1, weight=1)
        self.listen = tk.BooleanVar(value=self.settings["listen"])
        self.speak = tk.BooleanVar(value=self.settings["speak"])
        self.actions = tk.BooleanVar(value=self.settings["allow_actions"])
        toggles = ttk.Frame(options)
        toggles.grid(row=2, columnspan=2, sticky="w", pady=6)
        ttk.Checkbutton(toggles, text="Listen while unmuted", variable=self.listen).pack(side="left")
        ttk.Checkbutton(toggles, text="Spoken replies", variable=self.speak).pack(side="left", padx=12)
        ttk.Checkbutton(toggles, text="Explicit game commands", variable=self.actions).pack(side="left")
        ttk.Label(options, text="For spoken controls, begin with ‘HOLT’. Your physical mute button controls listening.", wraplength=790).grid(row=3, columnspan=2, sticky="w")
        ttk.Label(options, text="Preferences to remember").grid(row=4, column=0, sticky="w", pady=(8, 0))
        self.preferences = ttk.Entry(options)
        self.preferences.insert(0, self.settings["preferences"])
        self.preferences.grid(row=4, column=1, sticky="ew", pady=(8, 0))
        notebook = ttk.Notebook(frame, height=220)
        notebook.pack(fill="both", expand=True, pady=(12, 6))
        chat = ttk.Frame(notebook)
        details = ttk.Frame(notebook)
        cockpit = ttk.Frame(notebook)
        notebook.add(chat, text="Conversation")
        notebook.add(cockpit, text="Cockpit view")
        notebook.add(details, text="Service details")
        self.chat = tk.Text(chat, height=8, wrap="word", state="disabled", font=("Segoe UI", 11), padx=10, pady=10)
        self.chat.pack(fill="both", expand=True)
        self.details = tk.Text(details, height=8, wrap="word", state="disabled", font=("Consolas", 9))
        self.details.pack(fill="both", expand=True)
        self.cockpit = ttk.Label(cockpit, text="Game-window observations appear here when HOLT needs to look.")
        self.cockpit.pack(fill="both", expand=True)
        entry_row = ttk.Frame(frame)
        entry_row.pack(fill="x")
        self.entry = ttk.Entry(entry_row)
        self.entry.pack(side="left", fill="x", expand=True)
        self.entry.bind("<Return>", lambda event: self.send())
        ttk.Button(entry_row, text="Send", command=self.send).pack(side="right", padx=(8, 0))
        ttk.Label(frame, text="Start loads the models and connects to SKIPPY. Stop ends listening and this checkout’s local model services.", wraplength=850).pack(anchor="w", pady=(8, 0))
        self.lock = ROOT / "run/gui.json"
        self.lock.parent.mkdir(parents=True, exist_ok=True)
        self.lock.write_text(json.dumps({"pid": os.getpid(), "created": psutil.Process().create_time(), "exe": psutil.Process().exe()}))
        if Services().adopt_existing():
            self.state.set("Client stopped · local models already running")
            self.stop_button.configure(state="normal")
        window.protocol("WM_DELETE_WINDOW", self.close)
        self.window.after(100, self.poll)
        if autostart:
            self.window.after(300, self.start)

    @staticmethod
    def append(widget, text):
        widget.configure(state="normal")
        widget.insert("end", text + "\n\n")
        widget.see("end")
        widget.configure(state="disabled")

    def report(self, kind, value):
        self.events.put((kind, value))

    def collect_settings(self):
        self.settings.update(microphone=None if self.microphone.get() == "Windows default" else self.microphone.get(),
                             speaker=None if self.speaker.get() == "Windows default" else self.speaker.get(),
                             listen=self.listen.get(), speak=self.speak.get(), allow_actions=self.actions.get(),
                             preferences=self.preferences.get())
        save_settings(self.settings)

    def start(self):
        if self.worker and self.worker.is_alive():
            return
        self.collect_settings()
        self.stopped = threading.Event()
        self.start_button.configure(state="disabled")
        self.stop_button.configure(state="normal")
        self.state.set("Starting…")
        self.chat.configure(state="normal")
        self.chat.delete("1.0", "end")
        self.chat.configure(state="disabled")
        settings = dict(self.settings)

        def work():
            services = Services(lambda text: self.report("service", text))
            try:
                services.start(settings, self.stopped)
                if self.stopped.is_set():
                    raise InterruptedError()
                from client.engine import Engine
                self.engine = Engine(settings, self.report, self.stopped)
                asyncio.run(self.engine.run())
            except (asyncio.CancelledError, InterruptedError):
                pass
            except Exception as exc:
                from client.diagnostics import record
                self.report("error", record(exc))
            finally:
                self.stopped.set()
                services.stop()
                self.engine = None
                self.report("stopped", "Stopped")
        self.worker = threading.Thread(target=work, daemon=False)
        self.worker.start()

    def stop(self):
        self.state.set("Stopping…")
        self.stop_button.configure(state="disabled")
        self.stopped.set()
        if self.engine:
            self.engine.cancel()
        elif not self.worker or not self.worker.is_alive():
            # Also stop adopted benchmark services if the panel hasn't started yet.
            self.worker = threading.Thread(target=lambda: (Services(lambda text: self.report("service", text)).stop(), self.report("stopped", "Stopped")), daemon=False)
            self.worker.start()

    def send(self):
        text = self.entry.get().strip()
        if not self.engine:
            self.append(self.chat, "Start HOLT before sending a message.")
            return
        if text:
            self.engine.submit(text)
            self.entry.delete(0, "end")

    def new_session(self):
        if self.engine:
            self.engine.inputs.put(("new_session", None))
        else:
            from client.state import Memory
            memory = Memory()
            memory.new_session()
            memory.close()
            self.report("clear", "New conversation started; saved preferences retained.")

    def poll(self):
        start_request = ROOT / "run/start.request"
        if start_request.exists():
            start_request.unlink(missing_ok=True)
            self.start()
        request = ROOT / "run/stop.request"
        if request.exists():
            request.unlink(missing_ok=True)
            self.stop()
        while True:
            try:
                kind, value = self.events.get_nowait()
            except queue.Empty:
                break
            if kind == "microphone":
                self.microphone_status.set(value)
            elif kind == "state":
                self.state.set(value)
            elif kind == "ship":
                self.ship.set(value)
            elif kind in ("user", "assistant"):
                self.append(self.chat, ("Captain: " if kind == "user" else "HOLT: ") + value)
            elif kind == "clear":
                self.chat.configure(state="normal")
                self.chat.delete("1.0", "end")
                self.chat.configure(state="disabled")
                self.append(self.chat, value)
            elif kind == "stopped":
                self.microphone_status.set("Microphone stopped")
                self.state.set("Stopped")
                self.ship.set("Not connected")
                self.start_button.configure(state="normal")
                self.stop_button.configure(state="disabled")
            elif kind == "image":
                from PIL import Image, ImageTk
                picture = Image.open(io.BytesIO(base64.b64decode(value[0])))
                picture.thumbnail((820, 350))
                self.photo = ImageTk.PhotoImage(picture)
                self.cockpit.configure(image=self.photo, text="")
            elif kind == "error":
                self.append(self.chat, "System: " + value)
                self.append(self.details, value)
            else:
                self.append(self.details, str(value))
        if self.closing and (not self.worker or not self.worker.is_alive()):
            self.lock.unlink(missing_ok=True)
            self.window.destroy()
            return
        self.window.after(100, self.poll)

    def close(self):
        self.collect_settings()
        self.closing = True
        self.stop()


def main():
    logging.disable(logging.CRITICAL)
    parser = argparse.ArgumentParser()
    parser.add_argument("command", nargs="?", default="panel", choices=["panel", "start", "stop", "status"])
    args = parser.parse_args()
    run = ROOT / "run"
    run.mkdir(parents=True, exist_ok=True)
    gui_file = run / "gui.json"
    existing = json.loads(gui_file.read_text()) if gui_file.exists() else None
    if args.command == "status":
        records = json.loads((run / "services.json").read_text()) if (run / "services.json").exists() else {}
        print(json.dumps({name: matches(record) for name, record in records.items()}))
        return
    if args.command == "stop":
        if existing and matches(existing):
            (run / "stop.request").touch()
        Services().stop()
        return
    if existing and matches(existing):
        if args.command == "start":
            (run / "start.request").touch()
        else:
            messagebox.showinfo("HOLT", "The HOLT control window is already open.")
        return
    App(tk.Tk(), autostart=args.command == "start").window.mainloop()


if __name__ == "__main__":
    main()
