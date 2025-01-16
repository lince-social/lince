import { NextResponse } from "next/server";

export async function POST(request: Request) {
  try {
    const body = await request.json();

    if (!body.operation) {
      return NextResponse.json({
        success: false,
        message: `Empty operation: "${body}"; Quotes should be empty`,
      });
    }
    console.log(body.operation);

    return NextResponse.json(
      {
        success: true,
        message: `Successfully posted Operation: ${body.operation}`,
      },
      { status: 201 },
    );
  } catch (error) {
    console.log(`Error posting an 'Operation': ${error}`);
    return NextResponse.json(
      { success: false, message: `Error posting an 'Operation': ${error}` },
      { status: 500 },
    );
  }
}
