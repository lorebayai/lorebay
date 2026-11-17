"use strict";
(() => {
  const login = document.getElementById("login");
  const consent = document.getElementById("consent");
  const message = document.getElementById("message");
  const finish = document.getElementById("finish");
  const query = new URLSearchParams(window.location.search);
  const parameters = {};
  const fields = ["client_id", "redirect_uri", "response_type", "code_challenge", "code_challenge_method", "state", "resource"];
  let accessToken;
  let refreshToken;
  let isCli = false;
  let busy = false;
  let redirectUrl;

  for (const field of fields) {
    if (query.getAll(field).length > 1) {
      message.textContent = "This authorization link contains duplicate parameters.";
      login.hidden = true;
      return;
    }
    if (query.has(field)) parameters[field] = query.get(field);
  }

  async function request(path, method, body) {
    const headers = { "Content-Type": "application/json" };
    if (accessToken) headers.Authorization = `Bearer ${accessToken}`;
    const response = await fetch(`/api/v1/${path}`, {
      method, headers, body: JSON.stringify(body), credentials: "omit", cache: "no-store"
    });
    if (!response.ok) {
      if (response.status === 401 && path !== "login-session") {
        consent.hidden = true;
        login.hidden = false;
      }
      throw Object.assign(new Error("The request failed. Check the link or sign in again."), {
        status: response.status
      });
    }
    return (await response.json()).data;
  }

  async function logout() {
    if (!accessToken) return;
    try {
      await request("login-session", "DELETE", null);
    } catch (error) {
      if (error.status !== 401) throw error;
      await logoutExpiredSession();
    }
    accessToken = undefined;
    refreshToken = undefined;
  }

  async function logoutExpiredSession() {
    if (!refreshToken) return;
    try {
      const tokens = await request("login-session/refresh", "POST", { refresh_token: refreshToken });
      accessToken = tokens.access_token;
      refreshToken = tokens.refresh_token;
      await request("login-session", "DELETE", null);
    } catch (error) {
      if (error.status !== 401) throw error;
    }
  }

  async function followRedirect() {
    consent.hidden = true;
    finish.hidden = false;
    await logout();
    window.location.assign(redirectUrl);
  }

  login.addEventListener("submit", async (event) => {
    event.preventDefault();
    if (busy) return;
    busy = true;
    try {
      await logout();
      const values = new FormData(login);
      const tokens = await request("login-session", "POST", {
        username: values.get("username"), password: values.get("password")
      });
      accessToken = tokens.access_token;
      refreshToken = tokens.refresh_token;
      login.reset();
      const details = await request("authorization-request", "POST", parameters);
      isCli = details.default_agent_name === null;
      document.getElementById("client").textContent = details.client_name;
      document.getElementById("host").textContent = details.redirect_host;
      document.getElementById("purpose").textContent = isCli
        ? "This signs the lorebay CLI in as you."
        : "This authorizes an agent to act for you.";
      document.getElementById("agent").hidden = isCli;
      consent.elements.agent_name.disabled = isCli;
      consent.elements.agent_name.value = details.default_agent_name ?? "";
      login.hidden = true;
      consent.hidden = false;
      message.textContent = "Review this request before deciding.";
    } catch (error) {
      message.textContent = error.message;
      try { await logout(); } catch { message.textContent += " Session cleanup failed; reload to sign in again."; }
    } finally {
      busy = false;
    }
  });

  consent.addEventListener("submit", async (event) => {
    event.preventDefault();
    if (busy) return;
    busy = true;
    try {
      const body = { ...parameters, decision: event.submitter?.value ?? "approve" };
      if (!isCli && body.decision === "approve") body.agent_name = consent.elements.agent_name.value;
      const result = await request("authorization-decision", "POST", body);
      redirectUrl = result.redirect_url;
      await followRedirect();
    } catch (error) {
      message.textContent = error.message;
    } finally {
      busy = false;
    }
  });

  finish.addEventListener("click", async () => {
    if (busy) return;
    busy = true;
    try {
      await followRedirect();
    } catch (error) {
      message.textContent = error.message;
    } finally {
      busy = false;
    }
  });
})();
