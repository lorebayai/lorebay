"use strict";

(() => {
  interface Tokens {
    access_token: string;
    refresh_token: string;
  }

  interface AuthorizationDetails {
    client_name: string;
    redirect_host: string;
    default_agent_name: string | null;
  }

  interface DecisionResult {
    redirect_url: string;
  }

  interface ProblemDetails {
    errors?: { field: string; code: string }[];
  }

  class HttpError extends Error {
    constructor(
      readonly status: number,
      readonly problem: ProblemDetails = {},
    ) {
      super("The request failed. Check the link or sign in again.");
    }
  }

  const login = element("#login", HTMLFormElement);
  const consent = element("#consent", HTMLFormElement);
  const message = element("#message", HTMLParagraphElement);
  const finish = element("#finish", HTMLButtonElement);
  const agentName = element("#agent input", HTMLInputElement);
  const nameConflict = element("#name-conflict", HTMLDivElement);
  const chooseName = element("#choose-name", HTMLButtonElement);
  const query = new URLSearchParams(window.location.search);
  const parameters: Partial<Record<string, string>> = {};
  const fields = [
    "client_id",
    "redirect_uri",
    "response_type",
    "code_challenge",
    "code_challenge_method",
    "state",
    "resource",
  ];
  let accessToken: string | undefined;
  let refreshToken: string | undefined;
  let isCli = false;
  let busy = false;
  let redirectUrl: string | undefined;
  let conflictedName: string | undefined;

  for (const field of fields) {
    if (query.getAll(field).length > 1) {
      message.textContent =
        "This authorization link contains duplicate parameters.";
      login.hidden = true;
      return;
    }
    const value = query.get(field);
    if (value !== null) parameters[field] = value;
  }

  function element<T extends HTMLElement>(
    selector: string,
    kind: new () => T,
  ): T {
    const result = document.querySelector(selector);
    if (!(result instanceof kind))
      throw new Error("The authorization page is incomplete.");
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
      if (response.status === 401 && path !== "sign-in") {
        consent.hidden = true;
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
      await request("sign-in", "DELETE", null);
    } catch (error) {
      if (!(error instanceof HttpError) || error.status !== 401) throw error;
      await logoutExpiredSignIn();
    }
    accessToken = undefined;
    refreshToken = undefined;
  }

  async function logoutExpiredSignIn(): Promise<void> {
    if (!refreshToken) return;
    try {
      const tokens = await request<Tokens>("sign-in/refresh", "POST", {
        refresh_token: refreshToken,
      });
      accessToken = tokens.access_token;
      refreshToken = tokens.refresh_token;
      await request("sign-in", "DELETE", null);
    } catch (error) {
      if (!(error instanceof HttpError) || error.status !== 401) throw error;
    }
  }

  async function followRedirect(): Promise<void> {
    if (!redirectUrl) throw new Error("The redirect URL is missing.");
    consent.hidden = true;
    finish.hidden = false;
    await logout();
    window.location.assign(redirectUrl);
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
      const tokens = await request<Tokens>("sign-in", "POST", {
        username: values.get("username"),
        password: values.get("password"),
      });
      accessToken = tokens.access_token;
      refreshToken = tokens.refresh_token;
      login.reset();
      const details = await request<AuthorizationDetails>(
        "authorization-request",
        "POST",
        parameters,
      );
      isCli = details.default_agent_name === null;
      element("#client", HTMLElement).textContent = details.client_name;
      element("#host", HTMLElement).textContent = details.redirect_host;
      element("#purpose", HTMLElement).textContent = isCli
        ? "This signs the lorebay CLI in as you."
        : "This authorizes an agent to act for you.";
      element("#agent", HTMLElement).hidden = isCli;
      agentName.disabled = isCli;
      agentName.value = details.default_agent_name ?? "";
      conflictedName = undefined;
      nameConflict.hidden = true;
      login.hidden = true;
      consent.hidden = false;
      message.textContent = "Review this request before deciding.";
    } catch (error) {
      showError(error);
      try {
        await logout();
      } catch {
        message.textContent +=
          " Sign-in cleanup failed; reload to sign in again.";
      }
    } finally {
      busy = false;
    }
  }

  async function decide(submitter: HTMLElement | null): Promise<void> {
    if (busy) return;
    busy = true;
    const submittedName = agentName.value;
    try {
      const action =
        submitter instanceof HTMLButtonElement ? submitter.value : "approve";
      const body: typeof parameters & {
        decision: string;
        agent_name?: string;
        replace_existing?: boolean;
      } = {
        ...parameters,
        decision: action === "decline" ? "decline" : "approve",
      };
      if (!isCli && body.decision === "approve") {
        body.agent_name = submittedName;
        body.replace_existing =
          action === "replace" && conflictedName === submittedName;
      }
      const result = await request<DecisionResult>(
        "authorization-decision",
        "POST",
        body,
      );
      redirectUrl = result.redirect_url;
      await followRedirect();
    } catch (error) {
      if (
        error instanceof HttpError &&
        error.problem.errors?.some(
          (field) => field.field === "agent_name" && field.code === "taken",
        )
      ) {
        conflictedName = submittedName;
        nameConflict.hidden = agentName.value !== submittedName;
        message.textContent = "You already have an agent with this name.";
      } else {
        showError(error);
      }
    } finally {
      busy = false;
    }
  }

  async function retryRedirect(): Promise<void> {
    if (busy) return;
    busy = true;
    try {
      await followRedirect();
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

  consent.addEventListener("submit", (event) => {
    event.preventDefault();
    void decide(event.submitter);
  });

  agentName.addEventListener("input", () => {
    conflictedName = undefined;
    nameConflict.hidden = true;
  });

  chooseName.addEventListener("click", () => {
    conflictedName = undefined;
    nameConflict.hidden = true;
    agentName.focus();
    agentName.select();
    message.textContent = "Enter another agent name, then approve.";
  });

  finish.addEventListener("click", () => {
    void retryRedirect();
  });
})();
