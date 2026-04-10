/**
 * Example: Simple BPMNinja External Task Worker
 *
 * This worker subscribes to two topics and demonstrates:
 * - Basic task completion with variables
 * - Error handling with BPMN errors
 * - Automatic retry with exponential backoff
 * - Graceful shutdown on SIGINT/SIGTERM
 *
 * Run with: npx tsx example/simple-worker.ts
 */

import { ExternalTaskClient } from "../src/index.js";

// ---------------------------------------------------------------------------
// 1. Create the client
// ---------------------------------------------------------------------------

const client = new ExternalTaskClient({
  baseUrl: "http://localhost:8081",
  workerId: "demo-worker-01",
  lockDuration: 30_000,           // 30 seconds
  maxTasks: 5,                    // Fetch up to 5 tasks per poll
  asyncResponseTimeout: 10_000,   // Long-poll for 10 seconds
  pollingInterval: 500,           // 500ms between polls
  maxRetries: 3,                  // Retry failed handlers 3 times
  baseRetryDelay: 1_000,          // 1s → 2s → 4s backoff
  autoExtendLock: true,           // Extend lock for long tasks
  autoExtendLockInterval: 10_000, // Every 10 seconds
});

// ---------------------------------------------------------------------------
// 2. Subscribe to topics
// ---------------------------------------------------------------------------

// Topic: "send-email" — Simulates sending an email
client.subscribe("worker", async (task, service) => {

  console.log(`worker kontext: ${JSON.stringify(task.variables_snapshot)}`);

  // Simulate some async work (e.g. calling an email API)
  await new Promise((resolve) => setTimeout(resolve, 1_000));

  // Complete the task with result variables
  await service.complete({
    worker: "worker-01",
    emailSent: true,
    sentAt: new Date().toISOString(),
  });
});


// Topic: "send-email" — Simulates sending an email
client.subscribe("send-email", async (task, service) => {
  const { recipient, subject } = task.variables_snapshot as {
    recipient?: string;
    subject?: string;
  };

  console.log(`📧 Sending email to ${recipient ?? "unknown"}: ${subject ?? "no subject"}`);

  // Simulate some async work (e.g. calling an email API)
  await new Promise((resolve) => setTimeout(resolve, 1_000));

  // Complete the task with result variables
  await service.complete({
    emailSent: true,
    sentAt: new Date().toISOString(),
  });
});

// Topic: "validate-order" — Validates an order with possible BPMN error
client.subscribe("validate-order", async (task, service) => {
  const { orderId, amount } = task.variables_snapshot as {
    orderId?: string;
    amount?: number;
  };

  console.log(`📦 Validating order ${orderId ?? "unknown"} (amount: ${amount ?? 0})`);

  // Business validation: orders above 10,000 are rejected via BPMN error
  if (amount && amount > 10_000) {
    console.log(`❌ Order ${orderId} rejected: amount exceeds limit`);
    await service.bpmnError("ORDER_LIMIT_EXCEEDED");
    return;
  }

  // Simulate validation delay
  await new Promise((resolve) => setTimeout(resolve, 500));

  await service.complete({
    orderValid: true,
    validatedAt: new Date().toISOString(),
  });
});

// Topic: "flaky-task" — Demonstrates the retry mechanism
client.subscribe(
  "flaky-task",
  async (task, service) => {
    console.log(`🎲 Processing flaky task ${task.id}`);

    // Simulate a random failure (60% chance)
    if (Math.random() < 0.6) {
      throw new Error("Random transient failure — service temporarily unavailable");
    }

    await service.complete({ processed: true });
  },
  { maxRetries: 5 }, // Override global retry for this specific topic
);

// ---------------------------------------------------------------------------
// 3. Start polling
// ---------------------------------------------------------------------------

client.start();

// ---------------------------------------------------------------------------
// 4. Graceful Shutdown
// ---------------------------------------------------------------------------

async function shutdown(signal: string) {
  console.log(`\n🛑 Received ${signal}, shutting down gracefully...`);
  await client.stop();
  console.log("✅ Worker stopped. Goodbye!");
  process.exit(0);
}

process.on("SIGINT", () => shutdown("SIGINT"));
process.on("SIGTERM", () => shutdown("SIGTERM"));

console.log("🚀 BPMNinja worker is running. Press Ctrl+C to stop.\n");
