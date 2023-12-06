<script lang="ts">
  import svelteLogo from "./assets/svelte.svg";
  import viteLogo from "/vite.svg";
  import Counter from "./lib/Counter.svelte";
  import { onMount } from "svelte";

  let localSessionDescriptionElement: HTMLTextAreaElement;
  let remoteSessionDescriptionElement: HTMLTextAreaElement;
  let serverVideoElement: HTMLVideoElement;
  let canvasOneElement: HTMLCanvasElement;
  let canvasTwoElement: HTMLCanvasElement;
  let canvasThreeElement: HTMLCanvasElement;

  // Create peer conn
  const pc = new RTCPeerConnection({
    iceServers: [
      {
        // urls: "stun:stun.l.google.com:19302",
        urls: "stun:127.0.0.1:3478",
      },
    ],
  });

  let circlesOne = []; // Array for canvasOne
  let circlesTwo = []; // Array for canvasTwo
  let circlesThree = []; // Array for canvasThree

  onMount(() => {
    initCanvasStreams();
    startAnimation(canvasOneElement, "#006699", circlesOne);
    // startAnimation(canvasTwoElement, "#cf635f", circlesTwo);
    // startAnimation(canvasThreeElement, "#46c240", circlesThree);

    // Add click event listeners to canvases
    addClickListener(canvasOneElement, circlesOne);
    // addClickListener(canvasTwoElement, circlesTwo);
    // addClickListener(canvasThreeElement, circlesThree);
  });

  function addClickListener(canvas: HTMLCanvasElement, circlesArray: any[]) {
    canvas.addEventListener("click", (event) => {
      const rect = canvas.getBoundingClientRect();
      const x = event.clientX - rect.left;
      const y = event.clientY - rect.top;
      circlesArray.push({ x, y, radius: 10, color: "black" }); // Adjust radius and color as needed
    });
  }

  pc.oniceconnectionstatechange = (e) => {
    console.debug("connection state change", pc.iceConnectionState);
  };
  pc.onicecandidate = (event) => {
    if (event.candidate === null) {
      localSessionDescriptionElement.value = btoa(
        JSON.stringify(pc.localDescription)
      );
    }
  };

  pc.onnegotiationneeded = (e) =>
    pc
      .createOffer()
      .then((d) => pc.setLocalDescription(d))
      .catch(console.error);

  pc.ontrack = (event) => {
    console.log("Got track event", event);
    serverVideoElement.srcObject = new MediaStream([event.track]);
  };

  const initCanvasStreams = () => {
    const streams = [
      canvasOneElement.captureStream(10),
      // canvasTwoElement.captureStream(),
      // canvasThreeElement.captureStream(),
    ];

    streams.forEach((stream) =>
      stream.getVideoTracks().forEach((track) => pc.addTrack(track, stream))
    );
  };

  // const startAnimation = (
  //   canvas: HTMLCanvasElement,
  //   color: string,
  //   circles: any[],
  // ) => {
  //   const ctx = canvas.getContext("2d");
  //   if (!ctx) {
  //     return;
  //   }
  //   let angle = 0;
  //   const draw = () => {
  //     drawCircle(ctx, color, angle, circles);
  //     angle += Math.PI / 64;
  //     requestAnimationFrame(draw);
  //   };
  //   requestAnimationFrame(draw);
  // };

  const startAnimation = (
    canvas: HTMLCanvasElement,
    color: string,
    circles: any[]
  ) => {
    const ctx = canvas.getContext("2d");
    if (!ctx) {
      return;
    }

    let angle = 0;
    const fps = 10; // target frames per second
    const frameDuration = 1000 / fps; // Duration of each frame in milliseconds

    let lastDrawTime = Date.now();
    let frameCount = 0;
    let lastSecond = Date.now();
    let actualFps = 0;

    const draw = () => {
      const currentTime = Date.now();
      const timeSinceLastDraw = currentTime - lastDrawTime;

      if (timeSinceLastDraw >= frameDuration) {
        drawCircle(ctx, color, angle, circles);
        angle += Math.PI / 64;
        lastDrawTime = currentTime;
        frameCount++;

        // Check if a second has passed to update the FPS
        if (currentTime - lastSecond >= 1000) {
          actualFps = frameCount;
          frameCount = 0;
          lastSecond = currentTime;
          console.log("Actual FPS:", actualFps); // Log the actual FPS
        }
      }

      requestAnimationFrame(draw);
    };

    requestAnimationFrame(draw);
  };

  function drawCircle(
    ctx: CanvasRenderingContext2D,
    color: string,
    angle: number,
    circles: any[] = []
  ) {
    // Background
    ctx.clearRect(0, 0, 200, 200);
    ctx.fillStyle = "#eeeeee";
    ctx.fillRect(0, 0, 200, 200);
    // Draw and fill in circle
    ctx.beginPath();
    const radius = 25 + 50 * Math.abs(Math.cos(angle));
    ctx.arc(100, 100, radius, 0, Math.PI * 2, false);
    ctx.closePath();
    ctx.fillStyle = color;
    ctx.fill();

    // Redraw all small circles
    circles.forEach((circle) => {
      ctx.beginPath();
      ctx.arc(circle.x, circle.y, circle.radius, 0, Math.PI * 2, false);
      ctx.closePath();
      ctx.fillStyle = circle.color;
      ctx.fill();
    });
  }

  async function autoStartSession() {
    // make a post to localhost:7000/offer
    const body = JSON.stringify({
      offer: localSessionDescriptionElement.value,
    });

    // simply hit the /hello GET endpoint
    // const response = await fetch("/api/offer");
    // const data = await response.text();

    console.log("Sending offer to server");
    const response = await fetch("/api/offer", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify({
        name: localSessionDescriptionElement.value,
      }),
    });
    console.log("response", response);

    // const response = await fetch("/api/offer", {
    //   method: "POST",
    //   headers: {
    //     "Content-Type": "application/x-www-form-urlencoded",
    //   },
    //   body: "{}",
    // });

    const data = await response.text();

    // await pc.setRemoteDescription(
    //   new RTCSessionDescription(JSON.parse(atob(data.offer))),
    // );

    console.log("data", data);
  }

  async function startSession() {
    const sd = remoteSessionDescriptionElement.value;
    if (sd === "") {
      return alert("Session Description must not be empty");
    }

    try {
      await pc.setRemoteDescription(
        new RTCSessionDescription(JSON.parse(atob(sd)))
      );
      const answer = await pc.createAnswer();
      await pc.setLocalDescription(answer);
      localSessionDescriptionElement.value = btoa(
        JSON.stringify(pc.localDescription)
      );
    } catch (e) {
      alert(e);
    }
  }
</script>

<!-- Browser base64 Session Description<br />
<textarea id="localSessionDescription" readonly="true"></textarea> <br />

Golang base64 Session Description<br />
<textarea id="remoteSessionDescription"></textarea> <br />
<button onclick="window.startSession()"> Start Session </button><br /> -->

<textarea
  bind:this={localSessionDescriptionElement}
  id="localSessionDescription"
  readonly
></textarea> <br />
<textarea
  bind:this={remoteSessionDescriptionElement}
  id="remoteSessionDescription"
></textarea> <br />
<button on:click={autoStartSession}>Auto Start</button><br />
<button on:click={startSession}>Start Session</button><br />

<br />

<div style="display: flex">
  <canvas bind:this={canvasOneElement} id="canvasOne" height="200" width="200"
  ></canvas>
  <!-- <canvas bind:this={canvasTwoElement} id="canvasTwo" height="200" width="200"
  ></canvas>
  <canvas
    bind:this={canvasThreeElement}
    id="canvasThree"
    height="200"
    width="200"
  ></canvas> -->
</div>

Video from server<br />
<!-- <video id="serverVideo" width="200" height="200" autoplay muted></video> <br /> -->
<video
  bind:this={serverVideoElement}
  id="serverVideo"
  width="200"
  height="200"
  autoplay
  muted
></video>

<style>
  textarea {
    width: 500px;
    min-height: 75px;
  }
</style>
