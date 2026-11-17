"use strict";

(() => {
  interface Tokens {
    access_token: string;
    refresh_token: string;
  }
  interface DeviceDetails {
    user_code: string;
    created_at: number;
  }
  interface ProblemDetails {
    errors?: { field: string; code: string }[];
  }
  class HttpError extends Error {
    constructor(
      readonly status: number,
      readonly problem: ProblemDetails = {},
    ) {
      super(
        status === 429
          ? "Too many attempts. Wait before trying again."
          : "The request failed. Check the code or sign in again.",
      );
    }
  }
  const login = element("#login", HTMLFormElement);
  const code = element("#code", HTMLFormElement);
  const consent = element("#consent", HTMLFormElement);
  const message = element("#message", HTMLParagraphElement);
  const cancel = element("#cancel", HTMLButtonElement);
  const finish = element("#finish", HTMLButtonElement);
  const userCode = element("[name=user_code]", HTMLInputElement);
  let accessToken: string | undefined;
  let refreshToken: string | undefined;
  let confirmedCode: string | undefined;
  let busy = false;
  let completion = "Sign-in canceled.";
  userCode.value =
    new URLSearchParams(window.location.search).get("user_code") ?? "";

  function element<T extends HTMLElement>(
    selector: string,
    kind: new () => T,
  ): T {
    const result = document.querySelector(selector);
    if (!(result instanceof kind))
      throw new Error("The verification page is incomplete.");
    return result;
  }

  async function request<T>(
    path: string,
    method: string,
    body: unknown,
  ): Promise<T> {
    const headers: Record<string, string> = {
      "Content-Type": "application/json",
    };
    if (accessToken) headers.Authorization = `Bearer ${accessToken}`;
    const response = await fetch(`/api/v1/${path}`, {
      method,
      headers,
      body: JSON.stringify(body),
      credentials: "omit",
      cache: "no-store",
    });
    if (!response.ok) {
      if (response.status === 401 && path !== "login-session") {
        consent.hidden = true;
        code.hidden = true;
        login.hidden = false;
      }
      const problem = (await response
        .json()
        .catch(() => ({}))) as ProblemDetails;
      throw new HttpError(response.status, problem);
    }
    const envelope = (await response.json()) as { data: T };
    return envelope.data;
  }

  async function logout(): Promise<void> {
    if (!accessToken) return;
    try {
      await request("login-session", "DELETE", null);
    } catch (error) {
      if (!(error instanceof HttpError) || error.status !== 401) throw error;
      await logoutExpiredSession();
    }
    accessToken = undefined;
    refreshToken = undefined;
  }

  async function logoutExpiredSession(): Promise<void> {
    if (!refreshToken) return;
    try {
      const tokens = await request<Tokens>("login-session/refresh", "POST", {
        refresh_token: refreshToken,
      });
      accessToken = tokens.access_token;
      refreshToken = tokens.refresh_token;
      await request("login-session", "DELETE", null);
    } catch (error) {
      if (!(error instanceof HttpError) || error.status !== 401) throw error;
    }
  }

  function showError(error: unknown): void {
    message.textContent =
      error instanceof Error ? error.message : "The request failed.";
  }

  async function signIn(): Promise<void> {
    if (busy) return;
    busy = true;
    try {
      await logout();
      const values = new FormData(login);
      const tokens = await request<Tokens>("login-session", "POST", {
        username: values.get("username"),
        password: values.get("password"),
      });
      accessToken = tokens.access_token;
      refreshToken = tokens.refresh_token;
      login.reset();
      login.hidden = true;
      code.hidden = false;
      cancel.hidden = false;
      message.textContent =
        "Enter the code shown by your terminal, then review it.";
      userCode.focus();
    } catch (error) {
      showError(error);
    } finally {
      busy = false;
    }
  }

  async function review(): Promise<void> {
    if (busy) return;
    busy = true;
    try {
      const details = await request<DeviceDetails>(
        "device-sign-in-request",
        "POST",
        { user_code: userCode.value },
      );
      confirmedCode = details.user_code;
      element("#user-code", HTMLElement).textContent = details.user_code;
      element("#created-at", HTMLElement).textContent = new Date(
        details.created_at,
      ).toLocaleString();
      code.hidden = true;
      consent.hidden = false;
      message.textContent = "Check this request before deciding.";
    } catch (error) {
      showError(error);
    } finally {
      busy = false;
    }
  }

  async function complete(): Promise<void> {
    code.hidden = true;
    consent.hidden = true;
    login.hidden = true;
    cancel.hidden = true;
    finish.hidden = false;
    message.textContent = "Signing out of this page...";
    await logout();
    finish.hidden = true;
    message.textContent =
      completion + " This page is signed out. You may close it.";
  }

  async function decide(submitter: HTMLElement | null): Promise<void> {
    if (busy || !confirmedCode) return;
    busy = true;
    try {
      const decision =
        submitter instanceof HTMLButtonElement && submitter.value === "decline"
          ? "decline"
          : "approve";
      await request("device-sign-in-decision", "POST", {
        user_code: confirmedCode,
        decision,
      });
      completion =
        decision === "approve"
          ? "Sign-in approved. Return to your terminal."
          : "Sign-in declined.";
      await complete();
    } catch (error) {
      showError(error);
    } finally {
      busy = false;
    }
  }

  async function signOut(): Promise<void> {
    if (busy) return;
    busy = true;
    try {
      await complete();
    } catch (error) {
      showError(error);
    } finally {
      busy = false;
    }
  }

  login.addEventListener("submit", (event) => {
    event.preventDefault();
    void signIn();
  });
  code.addEventListener("submit", (event) => {
    event.preventDefault();
    void review();
  });
  consent.addEventListener("submit", (event) => {
    event.preventDefault();
    void decide(event.submitter);
  });
  cancel.addEventListener("click", () => {
    void signOut();
  });
  finish.addEventListener("click", () => {
    void signOut();
  });
})();
