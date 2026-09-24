use super::*;
use chrono::{DateTime, Duration, Utc};

fn in_range(date: &Value, until: &Value, range: &str) -> bool {
    let date = date
        .as_str()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok());
    let until = until
        .as_str()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok());
    date.zip(until).is_some_and(|(date, until)| {
        date <= until
            && match range {
                "week" => date >= until - Duration::days(7),
                "month" => date >= until - Duration::days(30),
                "year" => date >= until - Duration::days(365),
                "all" => true,
                _ => false,
            }
    })
}

impl Runtime {
    async fn backlog_context(&self, account: Option<&str>) -> Result<Value> {
        let account = account
            .ok_or_else(|| failure("account_required", "Choose an account"))?
            .to_owned();
        self.core
            .call(move |s| s.local_model_backlog_context(&account))
            .await
    }

    async fn backlog_counts(&self, context: &Value, cancel: &CancellationToken) -> Result<Value> {
        let mut after = 0;
        let (mut local, mut eligible, mut processed) = (0, 0, 0);
        let mut ranges = json!({"week":0,"month":0,"year":0,"all":0});
        loop {
            if cancel.is_cancelled() {
                return Err(failure("assistance_canceled", "Canceled"));
            }
            let capture = context.clone();
            let page = self
                .core
                .call(move |s| s.local_model_backlog_page(&capture, after))
                .await?;
            for item in page["items"].as_array().unwrap() {
                local += 1;
                if item["eligible"] != true || !in_range(&item["date"], &context["until"], "all") {
                    continue;
                }
                eligible += 1;
                if item["task"].is_null() {
                    processed += 1;
                } else {
                    for range in ["week", "month", "year", "all"] {
                        if in_range(&item["date"], &context["until"], range) {
                            ranges[range] = (ranges[range].as_u64().unwrap() + 1).into();
                        }
                    }
                }
            }
            if page["done"] == true {
                break;
            }
            after = page["after"].as_i64().unwrap();
            tokio::task::yield_now().await;
        }
        Ok(
            json!({"account":context["account"],"local_messages":local,"google_messages":null,
            "eligible":eligible,"processed":processed,"missing":eligible-processed,"ranges":ranges,
            "enabled":context["enabled"],"summaries":context["summaries"],"classification":context["classification"],
            "as_of":context["until"]}),
        )
    }

    pub(super) async fn backlog(
        self: &Arc<Self>,
        owner: &str,
        account: Option<&str>,
        cancel: CancellationToken,
    ) -> Result<Value> {
        let permit = self
            .slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| failure("assistance_busy", "Local model queue is full"))?;
        let context = self.backlog_context(account).await?;
        let id = fm_ipc::random_token();
        let token = cancel.child_token();
        {
            let mut jobs = self.jobs.lock().unwrap();
            if let Some(job) = jobs.values().find(|j| {
                j.owner == owner
                    && j.account.as_deref() == account
                    && j.state["kind"] == "coverage"
                    && j.state["status"] == "scanning"
                    && !j.cancel.is_cancelled()
            }) {
                return Ok(job.state.clone());
            }
            jobs.retain(|_, j| {
                !(j.owner == owner
                    && j.account.as_deref() == account
                    && j.state["kind"] == "coverage")
            });
            jobs.insert(
                id.clone(),
                Job {
                    owner: owner.into(),
                    account: account.map(str::to_owned),
                    cancel: token.clone(),
                    state: json!({"job_id":id,"kind":"coverage","status":"scanning"}),
                },
            );
        }
        let runtime = self.clone();
        let job_id = id.clone();
        tokio::spawn(async move {
            let _permit = permit;
            let mut state = match runtime.backlog_counts(&context, &token).await {
                Ok(counts) => counts,
                Err(e) => json!({"error":{"code":e.code,"message":e.message}}),
            };
            state["job_id"] = json!(job_id);
            state["kind"] = "coverage".into();
            state["status"] = if token.is_cancelled() {
                "canceled"
            } else if state.get("error").is_some() {
                "failed"
            } else {
                "completed"
            }
            .into();
            runtime.state(&job_id, state);
        });
        Ok(json!({"job_id":id,"status":"scanning"}))
    }

    pub(super) async fn start_backlog(
        self: &Arc<Self>,
        owner: &str,
        cancel: CancellationToken,
        account: Option<&str>,
        params: &Value,
    ) -> Result<Value> {
        let range = params["range"]
            .as_str()
            .filter(|v| ["week", "month", "year", "all"].contains(v))
            .ok_or_else(|| failure("invalid_request", "Choose week, month, year or all"))?
            .to_owned();
        let permit = self
            .slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| failure("assistance_busy", "Local model queue is full"))?;
        let context = self.backlog_context(account).await?;
        if context["enabled"] != true {
            return Err(failure(
                "assistance_disabled",
                "Select a local model and enable analysis for this account",
            ));
        }
        let id = fm_ipc::random_token();
        let token = cancel.child_token();
        let state = json!({"job_id":id,"kind":"backlog","account":account,"range":range,"status":"scanning",
            "completed":0,"skipped":0,"failed":0,"total":0,"processed":0,"error":null});
        {
            let mut jobs = self.jobs.lock().unwrap();
            if jobs.values().any(|j| {
                j.account.as_deref() == account
                    && j.state["kind"] == "backlog"
                    && matches!(
                        j.state["status"].as_str(),
                        Some("scanning" | "running" | "queued")
                    )
                    && !j.cancel.is_cancelled()
            }) {
                return Err(failure(
                    "assistance_busy",
                    "This account already has a backlog job",
                ));
            }
            jobs.retain(|_, j| !j.cancel.is_cancelled());
            if jobs.len() >= 32 {
                let old = jobs
                    .iter()
                    .find(|(_, j)| {
                        !matches!(
                            j.state["status"].as_str(),
                            Some("scanning" | "running" | "queued")
                        )
                    })
                    .map(|(id, _)| id.clone());
                if let Some(old) = old {
                    jobs.remove(&old);
                }
            }
            jobs.insert(
                id.clone(),
                Job {
                    owner: owner.into(),
                    account: account.map(str::to_owned),
                    cancel: token.clone(),
                    state: state.clone(),
                },
            );
        }
        let runtime = self.clone();
        let job_id = id.clone();
        tokio::spawn(async move {
            let _permit = permit;
            let mut state = state;
            let result = runtime
                .run_backlog(&job_id, &context, &range, &token, &mut state)
                .await;
            state["status"] = if token.is_cancelled() {
                "canceled"
            } else if result.is_err() {
                "failed"
            } else if state["failed"].as_u64().unwrap_or(0) > 0 {
                "completed_with_errors"
            } else {
                "completed"
            }
            .into();
            if let Err(e) = result {
                state["error"] = json!({"code":e.code,"message":e.message});
            }
            runtime.state(&job_id, state);
        });
        Ok(json!({"job_id":id,"status":"scanning"}))
    }

    async fn run_backlog(
        &self,
        id: &str,
        context: &Value,
        range: &str,
        token: &CancellationToken,
        state: &mut Value,
    ) -> Result<()> {
        let counts = self.backlog_counts(context, token).await?;
        state["total"] = counts["ranges"][range].clone();
        state["status"] = "running".into();
        self.state(id, state.clone());
        let mut after = 0;
        let started = Utc::now();
        loop {
            if token.is_cancelled() {
                return Err(failure("assistance_canceled", "Canceled"));
            }
            let capture = context.clone();
            let page = self
                .core
                .call(move |s| s.local_model_backlog_page(&capture, after))
                .await?;
            for item in page["items"].as_array().unwrap() {
                if item["eligible"] != true
                    || item["task"].is_null()
                    || !in_range(&item["date"], &context["until"], range)
                {
                    continue;
                }
                // One shared inference seat, released after each message so interactive work can run.
                let seat = tokio::select! {biased;_=token.cancelled()=>return Err(failure("assistance_canceled","Canceled")),
                p=self.active.clone().acquire_owned()=>p.map_err(|_|failure("engine_stopped","Engine stopped"))?};
                let capture = context.clone();
                let account = context["account"].as_str().unwrap().to_owned();
                let conversation = item["conversation_id"].as_str().unwrap().to_owned();
                let message = item["id"].as_str().unwrap().to_owned();
                let plan = self
                    .core
                    .call(move |s| {
                        s.local_model_backlog_validate(&capture)?;
                        if !s.local_model_backlog_eligible(&account, &message)? {
                            return Ok(None);
                        }
                        let Some(task) =
                            s.local_model_missing_task(&account, &conversation, &message)?
                        else {
                            return Ok(None);
                        };
                        s.local_model_prepare(&account, &conversation, &task, &message)
                            .map(Some)
                    })
                    .await;
                let result = match plan {
                    Ok(Some(plan)) => {
                        match ollama::infer(
                            &plan.settings,
                            &plan.task,
                            plan.payload.clone(),
                            &plan.categories,
                            &plan.sources,
                            token,
                        )
                        .await
                        {
                            Ok(output) => {
                                let stop = token.clone();
                                let capture = context.clone();
                                self.core
                                    .call(move |s| {
                                        if stop.is_cancelled() {
                                            return Err(failure("assistance_canceled", "Canceled"));
                                        }
                                        s.local_model_backlog_validate(&capture)?;
                                        if !s.local_model_backlog_eligible(
                                            &plan.account,
                                            &plan.target,
                                        )? {
                                            return Ok(false);
                                        }
                                        // Another owner may have finished this projection while we inferred.
                                        if s.local_model_missing_task(
                                            &plan.account,
                                            &plan.conversation,
                                            &plan.target,
                                        )?
                                        .as_deref()
                                            != Some(&plan.task)
                                        {
                                            return Ok(false);
                                        }
                                        s.local_model_finish(&plan, output)?;
                                        Ok(true)
                                    })
                                    .await
                            }
                            Err(e) => Err(e),
                        }
                    }
                    Ok(None) => Ok(false),
                    Err(e) => Err(e),
                };
                drop(seat);
                let counter = match result {
                    Ok(true) => "completed",
                    Ok(false) => "skipped",
                    Err(e) => {
                        if token.is_cancelled() {
                            return Err(e);
                        }
                        // A stopped runner or changed authority affects the entire remaining
                        // batch. Do not spend one timeout on every message in the mailbox.
                        if super::runner_failure(&e.code) {
                            return Err(e);
                        }
                        state["error"] = json!({"code":e.code,"message":e.message});
                        "failed"
                    }
                };
                state[counter] = (state[counter].as_u64().unwrap_or(0) + 1).into();
                state["processed"] = (state["processed"].as_u64().unwrap_or(0) + 1).into();
                // Concurrent work may change eligibility; never display progress beyond the total.
                state["total"] = state["total"]
                    .as_u64()
                    .unwrap_or(0)
                    .max(state["processed"].as_u64().unwrap())
                    .into();
                state["elapsed_seconds"] = (Utc::now() - started).num_seconds().into();
                self.state(id, state.clone());
                tokio::task::yield_now().await;
            }
            if page["done"] == true {
                break;
            }
            after = page["after"].as_i64().unwrap();
        }
        let skipped = state["total"]
            .as_u64()
            .unwrap_or(0)
            .saturating_sub(state["processed"].as_u64().unwrap_or(0));
        state["skipped"] = (state["skipped"].as_u64().unwrap_or(0) + skipped).into();
        state["processed"] = state["total"].clone();
        let capture = context.clone();
        self.core
            .call(move |s| s.local_model_backlog_validate(&capture))
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn backlog_ranges_are_bounded_and_exclude_future_or_invalid_dates() {
        let until = json!("2026-09-22T12:00:00Z");
        assert!(in_range(&json!("2026-09-15T12:00:00Z"), &until, "week"));
        assert!(!in_range(&json!("2026-09-15T11:59:59Z"), &until, "week"));
        assert!(in_range(
            &json!("2026-08-23T08:00:00-04:00"),
            &until,
            "month"
        ));
        assert!(!in_range(
            &json!("2026-08-23T07:59:59-04:00"),
            &until,
            "month"
        ));
        assert!(in_range(&json!("2025-09-22T12:00:00Z"), &until, "year"));
        assert!(!in_range(&json!("2025-09-22T11:59:59Z"), &until, "year"));
        assert!(in_range(&json!("2001-01-01T00:00:00Z"), &until, "all"));
        for date in [json!("2026-09-22T12:00:01Z"), json!("invalid"), Value::Null] {
            assert!(!in_range(&date, &until, "all"));
        }
    }
}
