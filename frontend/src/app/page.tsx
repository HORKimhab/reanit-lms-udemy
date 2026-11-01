import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { ApiStatus } from "@/components/layout/api-status";

const categories = [
  "Web Development",
  "Mobile Apps",
  "Data Science",
  "Cloud & DevOps",
  "Cyber Security",
  "UI/UX Design",
  "Business",
  "Languages",
];

export default function Home() {
  return (
    <main>
      <section className="border-b bg-muted/40">
        <div className="mx-auto max-w-7xl px-4 py-20">
          <h1 className="max-w-2xl text-4xl font-bold tracking-tight md:text-5xl">
            Learn skills that move your career forward
          </h1>
          <p className="mt-4 max-w-xl text-lg text-muted-foreground">
            Courses from expert instructors. Pay easily with Bakong KHQR or ABA, learn at your own pace.
          </p>
          <div className="mt-6">
            <ApiStatus />
          </div>
        </div>
      </section>

      <section className="mx-auto max-w-7xl px-4 py-12">
        <h2 className="mb-6 text-2xl font-semibold">Top categories</h2>
        <div className="grid grid-cols-2 gap-4 md:grid-cols-4">
          {categories.map((c) => (
            <Card key={c} className="transition hover:shadow-md">
              <CardHeader>
                <CardTitle className="text-base">{c}</CardTitle>
              </CardHeader>
              <CardContent className="text-sm text-muted-foreground">Explore courses</CardContent>
            </Card>
          ))}
        </div>
      </section>
    </main>
  );
}
