import hashlib
import time

from clients import Agent, Clients, require


class Journey:
    def __init__(self, clients):
        self.clients = clients
        self.space = None

    def setup(self, *, second=False, device=False, wrong=False):
        self.clients.login(0, wrong=wrong)
        self.space = self.clients.cli("space", "create", "--name", "Team work", "--summary", "Shared work")["key"]
        if second:
            self.clients.login(1, device=device)
            self.clients.cli("member", "add", self.space, self.username(1), "--role", "contributor")

    def username(self, person):
        return self.clients.people[person]["username"]

    def agent(self, name, person=0):
        return Agent(self.clients, person, name)

    def show(self, task, person=0):
        return self.clients.cli("task", "show", self.space, task["key"], person=person)


def plan_tasks_records_planners_and_claims_across_parties(journey):
    journey.setup(second=True)
    client, space = journey.clients, journey.space
    planner = journey.agent("Planner")
    worker = journey.agent("Worker", person=1)
    person_task = client.cli("task", "plan", space, "--title", "Prepare a report")
    agent_task = planner.call("create_task", space=space, title="Check the report", planned=True)
    planner.call("create_task", space=space, title="Collect report inputs")
    claimable = worker.call("list_tasks", space=space, claimable=True)["data"]
    require({row["task_id"] for row in claimable} == {person_task["task_id"], agent_task["task_id"]},
            "Worker did not find both planned tasks")
    for task, planner_name in ((person_task, None), (agent_task, planner.name)):
        planned = journey.show(task)
        require(planned["state"] == "planned" and planned["holder"] is None, "Task did not start planned")
        worker.call("claim_task", task_id=task["task_id"])
        shown = journey.show(task)
        require(shown["state"] == "working" and shown["person"] == journey.username(1)
                and shown["holder"] == worker.name and shown["planner"] == journey.username(0)
                and shown["planner_agent_name"] == planner_name, "Claim lost the holder, party or planner")
        refused = planner.call("claim_task", task_id=task["task_id"], failure="task.held")
        require(worker.name in refused["message"], "Second claimant was not told the holder")
        scratchpad = client.cli("task", "scratchpad", space, task["key"])
        planner.call("add_checkpoint", task_id=task["task_id"], content="Obsolete write",
                     failure="space.permission_required")
        planner.call("update_task", task_id=task["task_id"], result="Obsolete result",
                     failure="space.permission_required")
        require(journey.show(task, person=1) == shown, "Refused writes or claim changed the task")
        require(client.cli("task", "scratchpad", space, task["key"]) == scratchpad,
                "Another party changed the Scratchpad")
    worker.call("release_task", task_id=agent_task["task_id"])
    released = journey.show(agent_task)
    require(released["state"] == "planned" and released["person"] == journey.username(0)
            and released["holder"] is None and released["planner"] == journey.username(0)
            and released["planner_agent_name"] == planner.name, "Release lost the party or planner")
    planner.call("claim_task", task_id=agent_task["task_id"])
    planner.call("add_checkpoint", task_id=agent_task["task_id"], content="Started the review")
    shown = journey.show(agent_task)
    planner.call("release_task", task_id=agent_task["task_id"], failure="task.has_work")
    require(journey.show(agent_task) == shown, "Refused release changed the task")


def take_over_task_preserves_checkpoints_and_refuses_previous_holder(journey):
    journey.setup()
    first, second = journey.agent("First worker"), journey.agent("Continuing worker")
    task = first.call("create_task", space=journey.space, title="Continue a report")
    first.call("add_checkpoint", task_id=task["task_id"], content="Collected the inputs")
    first_file = first.call("create_file", task_id=task["task_id"], file_name="inputs.txt", summary="Report inputs")
    second.call("claim_task", task_id=task["task_id"])
    first.call("add_checkpoint", task_id=task["task_id"], content="Obsolete write", failure="task.not_holder")
    first.call("update_task", task_id=task["task_id"], result="Obsolete result", failure="task.not_holder")
    first.call("create_file", task_id=task["task_id"], file_name="obsolete.txt", summary="Obsolete file",
               failure="task.not_holder")
    second_file = second.call("create_file", task_id=task["task_id"], file_name="review.txt", summary="Reviewed inputs")
    for agent, file, payload in ((first, first_file, b"Collected inputs"), (second, second_file, b"Checked inputs")):
        journey.clients.upload(file, payload)
        agent.call("complete_file", task_id=task["task_id"], key=file["key"])
    second.call("add_checkpoint", task_id=task["task_id"], content="Checked the inputs")
    second.call("update_task", task_id=task["task_id"], result="Report checked")
    shown = journey.show(task)
    require(shown["holder"] == second.name and shown["result"] == "Report checked", "Takeover did not persist")
    require({(row["key"], row["agent_name"]) for row in shown["files"]} == {
        (first_file["key"], first.name), (second_file["key"], second.name),
    } and len(shown["files"]) == 2, "Takeover lost a file or its registering agent")
    scratchpad = journey.clients.cli("task", "scratchpad", journey.space, task["key"])
    require([(row["agent_name"], row["content"]) for row in scratchpad] == [
        (first.name, "Collected the inputs"), (second.name, "Checked the inputs"),
    ], "Scratchpad lost each writer or accepted the obsolete write")


def wait_for_person_resumes_after_request_approval(journey):
    journey.setup()
    agent = journey.agent("Waiting worker")
    task = agent.call("create_task", space=journey.space, title="Publish a guide")
    agent.call("update_task", task_id=task["task_id"], state="waiting", state_reason="Awaiting a person's answer")
    waiting = journey.show(task)
    require(waiting["state"] == "waiting" and waiting["state_reason"] == "Awaiting a person's answer",
            "Waiting reason was not visible to the person")
    require(waiting["state_changed_by"] == journey.username(0) and waiting["state_changed_agent_name"] == agent.name,
            "Waiting state did not name the person and agent who set it")
    agent.call("update_task", task_id=task["task_id"], state="working")
    require(journey.show(task)["state"] == "working", "Agent did not resume")
    request = agent.call("create_request", task_id=task["task_id"], action="create", name="guide.md",
                         summary="Team guide", content="Use the reviewed guide", wait=True)
    waiting = journey.show(task)
    require(waiting["state"] == "waiting" and request["key"] in waiting["state_reason"],
            "Task did not wait on its request")
    seen = journey.clients.cli("request", "show", journey.space, request["key"])
    journey.clients.cli("request", "approve", journey.space, request["key"], "--revision", seen["revision"])
    resumed = journey.show(task)
    require(resumed["state"] == "working" and resumed["holder"] == agent.name
            and resumed["state_reason"] is None, "Approval did not resume the held task")
    require(agent.call("get_request", space=journey.space, key=request["key"])["status"] == "approved",
            "Agent did not see the approval")
    guide = agent.call("read_knowledge", space=journey.space, name="guide.md")
    require(guide["content"] == "Use the reviewed guide", "Approved guide was not readable")
    withdrawn = agent.call("create_request", task_id=task["task_id"], action="change", target_key=guide["key"],
                           content="Unneeded addition", wait=True)
    require(journey.show(task)["state"] == "waiting", "Second request did not wait")
    seen = journey.clients.cli("request", "show", journey.space, withdrawn["key"])
    journey.clients.cli("request", "reject", journey.space, withdrawn["key"], "--revision", seen["revision"],
                        "--reason", "Withdraw the unneeded addition")
    resumed = journey.show(task)
    require(resumed["state"] == "working" and resumed["holder"] == agent.name and resumed["state_reason"] is None,
            "Withdrawal did not resume the held task")
    require(agent.call("get_request", space=journey.space, key=withdrawn["key"])["status"] == "rejected",
            "Agent did not see the withdrawal")


def end_tasks_preserves_outcomes_and_reopens_done(journey):
    journey.setup(second=True)
    agent = journey.agent("Finishing worker")
    for state, reason in (("done", None), ("failed", "Input is unavailable"), ("canceled", "Work is superseded")):
        task = agent.call("create_task", space=journey.space, title=f"Finish as {state}")
        changes = {"state": state}
        if reason:
            changes["state_reason"] = reason
        agent.call("update_task", task_id=task["task_id"], **changes)
        shown = journey.show(task)
        require(shown["state"] == state and shown["state_reason"] == reason
                and shown["holder"] is None and shown["ended_at"] is not None, "Final outcome was not recorded")
        if state == "done":
            agent.call("update_task", task_id=task["task_id"], state="working")
            reopened = journey.show(task)
            require(reopened["state"] == "working" and reopened["holder"] == agent.name
                    and reopened["ended_at"] is None, "Done task was not reopened")
        else:
            agent.call("update_task", task_id=task["task_id"], state="working", failure="task.state_invalid")
            require(journey.show(task) == shown, "Final task changed after refused reopening")
    other = journey.agent("Other worker", person=1)
    for person in (1, 0):
        task = other.call("create_task", space=journey.space, title="Stop obsolete work")
        reason = "Canceled by the person" if person else "Canceled by the space owner"
        journey.clients.cli("task", "cancel", journey.space, task["task_id"], "--reason", reason, person=person)
        shown = other.call("get_task", space=journey.space, key=task["key"])
        require(shown["state"] == "canceled" and shown["state_reason"] == reason
                and shown["state_changed_by"] == journey.username(person) and shown["holder"] is None,
                "Agent did not see the person's cancellation")


def update_result_preserves_history_and_refuses_stale_version(journey):
    journey.setup()
    first, second = journey.agent("Original writer"), journey.agent("Next writer")
    task = first.call("create_task", space=journey.space, title="Write a report")
    initial = first.call("update_task", task_id=task["task_id"], result="First report")
    second.call("claim_task", task_id=task["task_id"])
    current = second.call("update_task", task_id=task["task_id"], result="Reviewed report",
                          result_version=initial["result_version"])
    second.call("update_task", task_id=task["task_id"], result="Obsolete report", state="done",
                result_version=initial["result_version"], failure="task.result_changed")
    shown = journey.show(task)
    require(shown["result"] == "Reviewed report" and shown["result_version"] == current["result_version"]
            and shown["state"] == "working", "Stale result changed the task")
    history = journey.clients.cli("task", "results", journey.space, task["key"])
    require([(row["version"], row["result"], row["agent_name"], row["person"]) for row in history] == [
        (2, "Reviewed report", second.name, journey.username(0)),
        (1, "First report", first.name, journey.username(0)),
    ], "Person did not receive both result versions and writers")


def record_work_reads_back_files_and_revokes_agent(journey):
    journey.setup()
    client, space = journey.clients, journey.space
    agent = journey.agent("Recording worker")
    task = agent.call("create_task", space=space, title="Record a complete report")
    contents = ["Plan: collect the inputs and write a report", "Checked the report and its attachment"]
    for content in contents:
        agent.call("add_checkpoint", task_id=task["task_id"], content=content)
    payload = b"name,result\nreport,checked\n\x00\xff"
    file = agent.call("create_file", task_id=task["task_id"], file_name="report.csv", summary="Checked report")
    client.upload(file, payload)
    agent.call("complete_file", task_id=task["task_id"], key=file["key"])
    agent.call("update_task", task_id=task["task_id"], result="Report and attachment checked", state="done")
    require(any(row["task_id"] == task["task_id"] for row in client.cli("task", "list", space)),
            "Person did not find the recorded task")
    shown = journey.show(task)
    require(shown["state"] == "done" and shown["result"] == "Report and attachment checked"
            and len(shown["files"]) == 1 and shown["files"][0]["key"] == file["key"],
            "Person did not see the report and attachment")
    require([row["content"] for row in client.cli("task", "scratchpad", space, task["key"])] == contents,
            "Person did not read the full Scratchpad")
    url = client.cli("file", "url", space, file["key"])["url"]
    require(client.download(url) == (len(payload), hashlib.sha256(payload).hexdigest()), "Downloaded file bytes differ")
    require(agent.name in [row["name"] for row in client.cli("agent", "list")], "Agent sign-in was not listed")
    client.cli("agent", "revoke", agent.name, "--yes")
    agent.call("list_spaces", http_status=401)
    require(agent.name not in [row["name"] for row in client.cli("agent", "list")], "Revoked sign-in remains")


def revise_request_reconciles_outdated_entry_before_approval(journey):
    journey.setup(second=True)
    client, space = journey.clients, journey.space
    entry = client.cli("knowledge", "create", space, "guide.md", "--summary", "Guide", "--content", "Initial guide")
    writer, reader = journey.agent("Guide writer"), journey.agent("Guide reader", person=1)
    reviser = journey.agent("Guide reviser")
    task = writer.call("create_task", space=space, title="Revise the team guide")
    request = writer.call("create_request", task_id=task["task_id"], action="change", target_key=entry["key"],
                          content="Proposed guide")
    revised = client.cli("request", "revise", space, request["key"], "--content", "Person's proposal")
    seen = client.cli("request", "show", space, request["key"])
    require(seen["revision"] == revised["revision"] and seen["new_content"] == "Person's proposal"
            and seen["revised_by"] == journey.username(0) and seen["reviser_agent_name"] is None,
            "Person's revision was not visible")
    reader.call("revise_request", space=space, key=request["key"], content="Other party's proposal",
                failure="space.permission_required")
    require(client.cli("request", "show", space, request["key"]) == seen,
            "Another party changed the request")
    revised = reviser.call("revise_request", space=space, key=request["key"], content="Reviewed guide")
    seen = client.cli("request", "show", space, request["key"])
    require(seen["revision"] == revised["revision"] and seen["new_content"] == "Reviewed guide",
            "Person did not see the agent's revision")
    require(seen["requester_agent_name"] == writer.name and seen["reviser_agent_name"] == reviser.name,
            "Request did not name its submitting and last revising agents")
    client.cli("knowledge", "change", space, entry["key"], "--version", entry["version"], "--content", "Person's guide")
    outdated = client.cli("request", "show", space, request["key"])
    require(outdated["outdated"] is True, "Direct change did not mark the pending request outdated")
    listed = client.cli("request", "list", space)
    require(any(row["key"] == request["key"] and row["outdated"] is True for row in listed),
            "Person's request list did not mark the pending request outdated")
    refused = client.cli("request", "approve", space, request["key"], "--revision", outdated["revision"], failure={409})
    require(refused["code"] == "person_request.outdated", "Outdated approval was not refused")
    current = writer.call("read_knowledge", space=space, key=entry["key"])
    require(current["content"] == "Person's guide", "Outdated approval changed Knowledge")
    reconciled = "Person's guide with the reviewed addition"
    revised = reviser.call("revise_request", space=space, key=request["key"], content=reconciled)
    require(revised["was_outdated"] is True and revised["outdated"] is False,
            "Agent's revision did not report the outdated state it reconciled")
    seen = client.cli("request", "show", space, request["key"])
    require(seen["outdated"] is False and seen["revision"] > outdated["revision"],
            "Reconciled revision did not refresh the target snapshot")
    refused = client.cli("request", "approve", space, request["key"], "--revision", outdated["revision"], failure={409})
    require(refused["code"] == "person_request.revision_changed", "Stale decision was not refused")
    require(writer.call("read_knowledge", space=space, key=entry["key"]) == current,
            "Stale decision changed Knowledge")
    require(client.cli("request", "show", space, request["key"]) == seen, "Stale decision changed the request")
    client.cli("request", "approve", space, request["key"], "--revision", seen["revision"])
    require(reader.call("read_knowledge", space=space, key=entry["key"])["content"] == reconciled,
            "Another party's agent did not read the approved guide")
    require(writer.call("get_request", space=space, key=request["key"])["status"] == "approved",
            "Writer did not see the final approval")


def request_knowledge_requires_other_party_and_preserves_decisions(journey):
    journey.setup(second=True)
    client, space = journey.clients, journey.space
    client.login(2)
    client.cli("member", "add", space, journey.username(2), "--role", "viewer")
    client.cli("member", "set-role", space, journey.username(2), "--role", "admin")
    client.cli("space", "show", space, person=2)
    writer, reader = journey.agent("Proposing worker", person=1), journey.agent("Knowledge reader")
    task = writer.call("create_task", space=space, title="Share a reviewed guide")
    proposed = writer.call("create_request", task_id=task["task_id"], action="create", name="guide.md",
                           summary="Team guide", content="Approved guide")
    own = client.cli("request", "show", space, proposed["key"], person=1)
    refused = client.cli("request", "approve", space, proposed["key"], "--revision", own["revision"],
                         person=1, failure={403})
    require(refused["code"] == "person_request.own_party", "Contributor approved their own agent's request")
    seen = client.cli("request", "show", space, proposed["key"], person=2)
    client.cli("request", "approve", space, proposed["key"], "--revision", seen["revision"], person=2)
    entry = reader.call("read_knowledge", space=space, name="guide.md")
    require(entry["content"] == "Approved guide", "Third party did not read the approved entry")
    rejected = writer.call("create_request", task_id=task["task_id"], action="change", target_key=entry["key"],
                           content="Unreviewed guide")
    seen = client.cli("request", "show", space, rejected["key"], person=2)
    client.cli("request", "reject", space, rejected["key"], "--revision", seen["revision"],
               "--reason", "Keep the reviewed guide", person=2)
    decision = writer.call("get_request", space=space, key=rejected["key"])
    require(decision["status"] == "rejected" and decision["reject_reason"] == "Keep the reviewed guide",
            "Proposing agent did not read the rejection and its reason")
    entry = client.cli("knowledge", "show", space, entry["key"], person=2)
    changed = client.cli("knowledge", "change", space, entry["key"], "--version", entry["version"],
                         "--content", "Admin's guide", person=2)
    current = reader.call("read_knowledge", space=space, key=entry["key"])
    require(current["content"] == "Admin's guide", "Agent did not read the direct change")
    for operation in (("change", "--content", "Obsolete guide"), ("delete", "--yes"),
                      ("rollback", entry["version"])):
        refused = client.cli("knowledge", operation[0], space, entry["key"], *operation[1:],
                             "--version", entry["version"], person=2, failure={409})
        require(refused["code"] == "knowledge.version_changed", "Stale direct write was not refused")
        require(reader.call("read_knowledge", space=space, key=entry["key"]) == current,
                "Stale direct write changed Knowledge")
    client.cli("knowledge", "rollback", space, entry["key"], entry["version"],
               "--version", changed["version"], person=2)
    require(reader.call("read_knowledge", space=space, key=entry["key"])["content"] == "Approved guide",
            "Agent did not read the rollback")


def sign_in_enforces_space_roles_and_removal(journey):
    journey.setup(second=True, device=True, wrong=True)
    client, space = journey.clients, journey.space
    planned = client.cli("task", "plan", space, "--title", "Work awaiting a contributor")
    for role in ("viewer", "contributor", "admin"):
        client.cli("member", "set-role", space, journey.username(1), "--role", role)
        require(any(row["username"] == journey.username(1) and row["role"] == role
                    for row in client.cli("member", "list", space)), "Person did not see the new role")
        require(client.cli("space", "show", space, person=1)["key"] == space, "Member could not read the space")
        plan = ("task", "plan", space, "--title", f"Work as {role}")
        knowledge = ("knowledge", "create", space, f"{role}.md", "--summary", "Role guide", "--content", "Team guide")
        if role == "viewer":
            client.cli(*plan, person=1, failure={403})
            viewer = journey.agent("Viewing worker", person=1)
            before = journey.show(planned)
            viewer.call("claim_task", task_id=planned["task_id"], failure="space.permission_required")
            require(journey.show(planned) == before and before["state"] == "planned",
                    "Viewer's claim changed the planned task")
        else:
            task = client.cli(*plan, person=1)
            require(journey.show(task)["planner"] == journey.username(1), "Contributor could not plan work")
        if role == "admin":
            entry = client.cli(*knowledge, person=1)
            require(client.cli("knowledge", "show", space, entry["key"])["content"] == "Team guide",
                    "Admin's Knowledge change was not visible")
        else:
            client.cli(*knowledge, person=1, failure={403})
    client.cli("member", "remove", space, journey.username(1))
    require(not any(row["username"] == journey.username(1) for row in client.cli("member", "list", space)),
            "Removed person is still a member")
    client.cli("space", "show", space, person=1, failure={403, 404})
    client.cli("task", "plan", space, "--title", "Removed person's work", person=1, failure={403, 404})


JOURNEYS = {
    1: plan_tasks_records_planners_and_claims_across_parties,
    2: take_over_task_preserves_checkpoints_and_refuses_previous_holder,
    3: wait_for_person_resumes_after_request_approval,
    4: end_tasks_preserves_outcomes_and_reopens_done,
    5: update_result_preserves_history_and_refuses_stale_version,
    6: revise_request_reconciles_outdated_entry_before_approval,
    7: record_work_reads_back_files_and_revokes_agent,
    8: request_knowledge_requires_other_party_and_preserves_decisions,
    9: sign_in_enforces_space_roles_and_removal,
}


def run_journeys(origin, people, root, *, executable="lorebay", authority=None, chromium=None,
                 numbers=None, credentials=None):
    for number in numbers or people:
        started = time.monotonic()
        clients = Clients(origin, people[number], root / str(number), executable, authority, chromium)
        try:
            JOURNEYS[number](Journey(clients))
            print(f"PASS journey_{number} {JOURNEYS[number].__name__} ({time.monotonic() - started:.1f}s)", flush=True)
        except Exception as error:
            raise RuntimeError(f"Journey {number}: {clients.redact(str(error))}") from None
        finally:
            if credentials is not None:
                credentials.extend(clients.secrets)
            clients.close()
