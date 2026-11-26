// Red Noir: the world in black, white and grey; only blood keeps its red.
// Uniforms (GLDEFS, all int): mode, tics (35 per second), contrast, grain
// and vignette in percent. GZDoom / UZDoom post-process GLSL; Room
// translates it to WGSL.
//
// A post-process shader cannot ask what a pixel shows, so blood is told by
// its colour: Doom's blood is the palette's pure red ramp (red high, green
// and blue near zero), while red bricks, flesh and fire are browner, pinker
// or more orange, and turn grey. Mode 2 lets every strong red through
// (keys, red lights), mode 3 none.

float luma(vec3 c)
{
	return dot(c, vec3(0.299, 0.587, 0.114));
}

vec3 rgb2hsv(vec3 c)
{
	vec4 k = vec4(0.0, -1.0 / 3.0, 2.0 / 3.0, -1.0);
	vec4 p = mix(vec4(c.bg, k.wz), vec4(c.gb, k.xy), step(c.b, c.g));
	vec4 q = mix(vec4(p.xyw, c.r), vec4(c.r, p.yzx), step(p.x, c.r));
	float d = q.x - min(q.w, q.y);
	float e = 1.0e-6;
	return vec3(abs(q.z + (q.w - q.y) / (6.0 * d + e)), d / (q.x + e), q.x);
}

// 1 where the pixel is blood red, 0 elsewhere, soft in between.
// width: how far from pure red the hue may be (degrees); minSat: how
// saturated it must be.
float redMask(vec3 c, float width, float minSat)
{
	vec3 hsv = rgb2hsv(c);
	float d = min(hsv.x, 1.0 - hsv.x) * 360.0;
	float hue = 1.0 - smoothstep(width * 0.6, width, d);
	float sat = smoothstep(minSat - 0.08, minSat + 0.06, hsv.y);
	float val = smoothstep(0.05, 0.12, hsv.z);
	// Red must clearly lead green and blue (not brown, not pink).
	float lead = smoothstep(0.08, 0.2, c.r - max(c.g, c.b));
	return hue * sat * val * lead;
}

float hash(vec2 p)
{
	return fract(sin(dot(p, vec2(12.9898, 78.233))) * 43758.5453);
}

void main()
{
	vec4 src = texture(InputTexture, TexCoord);
	vec3 c = src.rgb;
	float k = float(contrast) / 100.0;
	float l = clamp((luma(c) - 0.5) * k + 0.5, 0.0, 1.0);

	float mask = 0.0;
	if (mode == 1) mask = redMask(c, 18.0, 0.62);
	else if (mode == 2) mask = redMask(c, 30.0, 0.42);

	// Blood: its own red, a little deeper and brighter against the grey.
	vec3 blood = vec3(clamp(max(c.r, l) * 1.12, 0.0, 1.0), c.g * 0.5, c.b * 0.5);
	vec3 o = mix(vec3(l), blood, mask);

	// Dark corners.
	vec2 d = TexCoord - 0.5;
	float v = 1.0 - float(vignette) / 100.0 * smoothstep(0.25, 0.75, length(d * vec2(1.25, 1.0)));
	o *= v;

	// Film grain, new every tic, on the greys only.
	vec2 size = vec2(textureSize(InputTexture, 0));
	float n = hash(floor(TexCoord * size) + vec2(float(tics) * 7.0, float(tics) * 3.0)) - 0.5;
	o += vec3(n * float(grain) / 100.0 * 0.12 * (1.0 - mask));

	FragColor = vec4(clamp(o, 0.0, 1.0), src.a);
}
